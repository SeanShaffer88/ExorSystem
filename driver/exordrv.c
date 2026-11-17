/*
 * ExorSystem kernel component.
 *
 * Registry visibility via CmRegisterCallbackEx, delivered to user mode through a minifilter
 * communication port. The filter registers no file-system callbacks: file changes are read
 * from the NTFS USN journal in user mode (crates/exor-win).
 *
 * Requires Windows 10 2004+ (ExAllocatePool2). Build with the WDK; see driver/README.md.
 * SECURITY: the port is restricted to SYSTEM and Administrators. Messages are best-effort:
 * if no client is connected or it is slow, events are counted as dropped, never queued
 * unboundedly and never blocking the registry operation for more than 10 ms.
 */
#include <fltKernel.h>
#include <ntstrsafe.h>
#include "shared\exor_proto.h"

#define EXOR_TAG 'rxEx'
/* Development altitude. Obtain a Microsoft-assigned altitude (Activity Monitor range 360000-389999) before shipping. */
#define EXOR_ALTITUDE L"385201"
#define EXOR_SEND_TIMEOUT_100NS (-100000LL) /* 10 ms relative */

NTKERNELAPI PUCHAR PsGetProcessImageFileName(_In_ PEPROCESS Process);

static PFLT_FILTER gFilter = NULL;
static PFLT_PORT gServerPort = NULL;
static PFLT_PORT gClientPort = NULL;
static EX_PUSH_LOCK gPortLock;
static LARGE_INTEGER gCookie;
static volatile LONG64 gDropped = 0;

DRIVER_INITIALIZE DriverEntry;
static NTSTATUS ExorUnload(_In_ FLT_FILTER_UNLOAD_FLAGS Flags);
static NTSTATUS ExorConnect(_In_ PFLT_PORT ClientPort, _In_opt_ PVOID ServerPortCookie, _In_reads_bytes_opt_(Size) PVOID Context, _In_ ULONG Size, _Outptr_result_maybenull_ PVOID *ConnectionCookie);
static VOID ExorDisconnect(_In_opt_ PVOID ConnectionCookie);
static NTSTATUS ExorRegistryCallback(_In_ PVOID Context, _In_opt_ PVOID Arg1, _In_opt_ PVOID Arg2);

static const FLT_REGISTRATION gRegistration = {
    sizeof(FLT_REGISTRATION), FLT_REGISTRATION_VERSION, 0,
    NULL,   /* no context */
    NULL,   /* no operation callbacks */
    ExorUnload, NULL, NULL, NULL, NULL, NULL, NULL, NULL
};

/* Send one registry message. Safe to call at PASSIVE_LEVEL only (registry callbacks run there). */
static VOID ExorSend(USHORT Op, PCUNICODE_STRING Key, PCUNICODE_STRING Value)
{
    ULONG keyBytes = Key ? Key->Length : 0;
    ULONG valBytes = Value ? Value->Length : 0;
    PUCHAR procName = PsGetProcessImageFileName(PsGetCurrentProcess());
    ULONG procBytes = 0;
    ULONG total;
    PUCHAR buf, p;
    LARGE_INTEGER timeout;
    EXOR_REG_MSG *hdr;
    NTSTATUS status;

    if (procName) { while (procBytes < 15 && procName[procBytes]) procBytes++; }
    if (keyBytes > 0xFFFE) keyBytes = 0xFFFE;
    if (valBytes > 0xFFFE) valBytes = 0xFFFE;
    keyBytes &= ~1u; valBytes &= ~1u;
    total = sizeof(EXOR_REG_MSG) + keyBytes + valBytes + procBytes;

    buf = (PUCHAR)ExAllocatePool2(POOL_FLAG_PAGED, total, EXOR_TAG);
    if (!buf) { InterlockedIncrement64(&gDropped); return; }

    hdr = (EXOR_REG_MSG *)buf;
    hdr->TotalLen = total;
    hdr->Kind = EXOR_KIND_REGISTRY;
    hdr->Op = Op;
    KeQuerySystemTimePrecise((PLARGE_INTEGER)&hdr->FileTime);
    hdr->Pid = HandleToULong(PsGetCurrentProcessId());
    hdr->KeyBytes = (USHORT)keyBytes;
    hdr->ValueBytes = (USHORT)valBytes;
    hdr->ProcBytes = (USHORT)procBytes;
    hdr->Reserved = 0;
    p = buf + sizeof(EXOR_REG_MSG);
    if (keyBytes) { RtlCopyMemory(p, Key->Buffer, keyBytes); p += keyBytes; }
    if (valBytes) { RtlCopyMemory(p, Value->Buffer, valBytes); p += valBytes; }
    if (procBytes) { RtlCopyMemory(p, procName, procBytes); }

    timeout.QuadPart = EXOR_SEND_TIMEOUT_100NS;
    FltAcquirePushLockShared(&gPortLock);
    if (gClientPort) {
        status = FltSendMessage(gFilter, &gClientPort, buf, total, NULL, NULL, &timeout);
        if (!NT_SUCCESS(status) || status == STATUS_TIMEOUT) InterlockedIncrement64(&gDropped);
    }
    FltReleasePushLock(&gPortLock);
    ExFreePoolWithTag(buf, EXOR_TAG);
}

static NTSTATUS ExorRegistryCallback(_In_ PVOID Context, _In_opt_ PVOID Arg1, _In_opt_ PVOID Arg2)
{
    REG_NOTIFY_CLASS cls = (REG_NOTIFY_CLASS)(ULONG_PTR)Arg1;
    PCUNICODE_STRING keyName = NULL;
    PVOID object = NULL;
    PCUNICODE_STRING valueName = NULL;
    USHORT op = 0;
    UNREFERENCED_PARAMETER(Context);

    if (!Arg2 || !gClientPort) return STATUS_SUCCESS; /* cheap early-out when nobody listens */

    switch (cls) {
    case RegNtPostCreateKeyEx: {
        PREG_POST_OPERATION_INFORMATION post = (PREG_POST_OPERATION_INFORMATION)Arg2;
        if (!NT_SUCCESS(post->Status)) return STATUS_SUCCESS;
        object = post->Object; op = EXOR_OP_CREATE_KEY;
        break;
    }
    case RegNtPostSetValueKey: {
        PREG_POST_OPERATION_INFORMATION post = (PREG_POST_OPERATION_INFORMATION)Arg2;
        PREG_SET_VALUE_KEY_INFORMATION pre = (PREG_SET_VALUE_KEY_INFORMATION)post->PreInformation;
        if (!NT_SUCCESS(post->Status) || !pre) return STATUS_SUCCESS;
        object = post->Object; valueName = pre->ValueName; op = EXOR_OP_SET_VALUE;
        break;
    }
    case RegNtPostDeleteValueKey: {
        PREG_POST_OPERATION_INFORMATION post = (PREG_POST_OPERATION_INFORMATION)Arg2;
        PREG_DELETE_VALUE_KEY_INFORMATION pre = (PREG_DELETE_VALUE_KEY_INFORMATION)post->PreInformation;
        if (!NT_SUCCESS(post->Status) || !pre) return STATUS_SUCCESS;
        object = post->Object; valueName = pre->ValueName; op = EXOR_OP_DELETE_VALUE;
        break;
    }
    case RegNtPreDeleteKey: { /* reported as an attempt; the key is gone by the post callback */
        PREG_DELETE_KEY_INFORMATION pre = (PREG_DELETE_KEY_INFORMATION)Arg2;
        object = pre->Object; op = EXOR_OP_DELETE_KEY;
        break;
    }
    default:
        return STATUS_SUCCESS;
    }

    if (object && NT_SUCCESS(CmCallbackGetKeyObjectIDEx(&gCookie, object, NULL, &keyName, 0))) {
        ExorSend(op, keyName, valueName);
        CmCallbackReleaseKeyObjectIDEx(keyName);
    }
    return STATUS_SUCCESS; /* observe only: never block or alter registry operations */
}

static NTSTATUS ExorConnect(_In_ PFLT_PORT ClientPort, _In_opt_ PVOID ServerPortCookie, _In_reads_bytes_opt_(Size) PVOID Context, _In_ ULONG Size, _Outptr_result_maybenull_ PVOID *ConnectionCookie)
{
    NTSTATUS status = STATUS_SUCCESS;
    UNREFERENCED_PARAMETER(ServerPortCookie); UNREFERENCED_PARAMETER(Context); UNREFERENCED_PARAMETER(Size);
    *ConnectionCookie = NULL;
    FltAcquirePushLockExclusive(&gPortLock);
    if (gClientPort) status = STATUS_ALREADY_REGISTERED; /* single client */
    else gClientPort = ClientPort;
    FltReleasePushLock(&gPortLock);
    return status;
}

static VOID ExorDisconnect(_In_opt_ PVOID ConnectionCookie)
{
    PFLT_PORT port;
    UNREFERENCED_PARAMETER(ConnectionCookie);
    FltAcquirePushLockExclusive(&gPortLock);
    port = gClientPort;
    gClientPort = NULL;
    FltReleasePushLock(&gPortLock);
    if (port) FltCloseClientPort(gFilter, &port);
}

static NTSTATUS ExorUnload(_In_ FLT_FILTER_UNLOAD_FLAGS Flags)
{
    UNREFERENCED_PARAMETER(Flags);
    CmUnRegisterCallback(gCookie);
    if (gServerPort) FltCloseCommunicationPort(gServerPort);
    FltUnregisterFilter(gFilter);
    FltDeletePushLock(&gPortLock);
    return STATUS_SUCCESS;
}

NTSTATUS DriverEntry(_In_ PDRIVER_OBJECT DriverObject, _In_ PUNICODE_STRING RegistryPath)
{
    NTSTATUS status;
    PSECURITY_DESCRIPTOR sd = NULL;
    OBJECT_ATTRIBUTES oa;
    UNICODE_STRING portName = RTL_CONSTANT_STRING(EXOR_PORT_NAME);
    UNICODE_STRING altitude = RTL_CONSTANT_STRING(EXOR_ALTITUDE);
    UNREFERENCED_PARAMETER(RegistryPath);

    FltInitializePushLock(&gPortLock);
    status = FltRegisterFilter(DriverObject, &gRegistration, &gFilter);
    if (!NT_SUCCESS(status)) { FltDeletePushLock(&gPortLock); return status; }

    status = FltBuildDefaultSecurityDescriptor(&sd, FLT_PORT_ALL_ACCESS); /* SYSTEM + Administrators */
    if (NT_SUCCESS(status)) {
        InitializeObjectAttributes(&oa, &portName, OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE, NULL, sd);
        status = FltCreateCommunicationPort(gFilter, &gServerPort, &oa, NULL, ExorConnect, ExorDisconnect, NULL, 1);
        FltFreeSecurityDescriptor(sd);
    }
    if (NT_SUCCESS(status)) {
        status = CmRegisterCallbackEx(ExorRegistryCallback, &altitude, DriverObject, NULL, &gCookie, NULL);
    }
    if (NT_SUCCESS(status)) {
        status = FltStartFiltering(gFilter);
        if (!NT_SUCCESS(status)) CmUnRegisterCallback(gCookie);
    }
    if (!NT_SUCCESS(status)) {
        if (gServerPort) FltCloseCommunicationPort(gServerPort);
        FltUnregisterFilter(gFilter);
        FltDeletePushLock(&gPortLock);
    }
    return status;
}
