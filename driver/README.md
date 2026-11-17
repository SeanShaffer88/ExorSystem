# ExorSystem driver

Minifilter-framework driver providing **registry visibility** through `CmRegisterCallbackEx`, delivered to user mode over the communication port `\ExorSystemPort`. File changes are *not* handled here: user mode reads the NTFS USN journal (`FSCTL_READ_USN_JOURNAL`) in `crates/exor-win`, which cannot be bypassed by user-space hooks.

The driver only observes: its registry callback always returns `STATUS_SUCCESS`, never alters or blocks an operation, and sends each message with a 10 ms cap, counting drops when no client is connected or the client is slow.

Status: **source only, not yet compiled or loaded** (see `AUDIT.md`).

## Wire format
`shared/exor_proto.h` mirrors `crates/exor-win/src/proto.rs`; the Rust parser is unit-tested.

## Build (WDK)
1. Install Visual Studio 2022 and the matching Windows Driver Kit.
2. New project: *Filter Driver: Filesystem Mini-Filter* (x64). Remove the template `.c`/`.inf`, add `exordrv.c`, `shared\exor_proto.h`, `exordrv.inf`.
3. Target OS: Windows 10 2004 or later (uses `ExAllocatePool2`). Build `Release|x64`.

## Test-load (development VM only)
```
bcdedit /set testsigning on        # reboot
sc create ExorDrv type= filesys binPath= C:\path\exordrv.sys
fltmc load ExorDrv
```
Unload with `fltmc unload ExorDrv`.

## Altitude
`385201` is a development placeholder inside the Activity Monitor range (360000–389999). Request a Microsoft-assigned altitude and update `EXOR_ALTITUDE` and `exordrv.inf` before distributing.

## Signing
Production loading requires an attested/WHQL-signed driver. Do not disable Secure Boot on machines you care about.
