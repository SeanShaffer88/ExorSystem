/*
 * ExorSystem driver <-> user-mode wire format. Mirror of crates/exor-win/src/proto.rs.
 * Little-endian, packed. Payload after the header: key (UTF-16LE) | value (UTF-16LE) | process (ASCII).
 */
#pragma once

#define EXOR_PORT_NAME      L"\\ExorSystemPort"
#define EXOR_KIND_REGISTRY  1
#define EXOR_OP_CREATE_KEY  1
#define EXOR_OP_DELETE_KEY  2
#define EXOR_OP_SET_VALUE   3
#define EXOR_OP_DELETE_VALUE 4

#pragma pack(push, 1)
typedef struct _EXOR_REG_MSG {
    ULONG      TotalLen;    /* header + payload */
    USHORT     Kind;        /* EXOR_KIND_REGISTRY */
    USHORT     Op;          /* EXOR_OP_* */
    ULONGLONG  FileTime;    /* 100 ns ticks since 1601 UTC */
    ULONG      Pid;
    USHORT     KeyBytes;
    USHORT     ValueBytes;
    USHORT     ProcBytes;
    USHORT     Reserved;
} EXOR_REG_MSG;             /* 28 bytes */
#pragma pack(pop)

C_ASSERT(sizeof(EXOR_REG_MSG) == 28);
