# Experimental ExorSystem registry driver

Minifilter-framework driver providing **registry visibility** through `CmRegisterCallbackEx`, delivered to user mode over the communication port `\ExorSystemPort`. File changes are *not* handled here: user mode reads the NTFS USN journal (`FSCTL_READ_USN_JOURNAL`) in `crates/exor-win`, as a separate optional library source, not as the desktop Watch backend.

The driver only observes: its registry callback always returns `STATUS_SUCCESS`, never alters or blocks an operation, and is designed to wait at most 10 ms on message delivery. Delivery is best effort and latency/loss behavior has not been measured.

**Lab status: WDK build/load and native capture Not run.** Neither the driver nor the optional USN reader powers Watch in the released desktop. See the [live-source gates](../docs/LIVE-COLLECTION.md#distribution-gates-open) and [audit](../AUDIT.md). Desktop use does not require a driver or administrator rights.

## Wire format
[`shared/exor_proto.h`](shared/exor_proto.h) mirrors the message layout read by [`exor-win/src/proto.rs`](../crates/exor-win/src/proto.rs). Rust parser fixtures are automated; they do not test a running driver.

## Build (WDK)
1. Install Visual Studio 2022 and the matching Windows Driver Kit.
2. New project: *Filter Driver: Filesystem Mini-Filter* (x64). Remove the template `.c`/`.inf`, add `exordrv.c`, `shared\exor_proto.h`, `exordrv.inf`.
3. Target OS: Windows 10 2004 or later (uses `ExAllocatePool2`). Build `Release|x64`.

## Test-load (disposable development VM only)
Run `scripts\dev-load.ps1 -Sys <path>\exordrv.sys` from an elevated PowerShell. It enables test signing (`bcdedit /set testsigning on`, reboot once), creates and trusts a self-signed certificate, signs the driver with `signtool`, registers the minifilter and runs `fltmc load ExorDrv`. `-Unload` attempts to unload the driver and remove its service/files. It does not revert test signing or certificate trust; revert the VM snapshot. The script is **unverified** (not yet run).

### Troubleshooting
| Symptom | Cause | Fix |
|---|---|---|
| `STATUS_DRIVER_BLOCKED_MANDATORY_SECURITY` / 0xC0000428 / "digital signature cannot be verified" | Not signed, cert not trusted, or test signing off | run the script; confirm `bcdedit /enum {current}` shows `testsigning Yes` after reboot |
| `bcdedit` reports Secure Boot policy | The VM does not allow test signing | Record the blocked lab check; use an isolated signing-test VM or an appropriately signed build |
| Load fails with Memory Integrity enabled | Possible driver compatibility issue | Record Code Integrity logs; do not weaken end-user protections |
| `fltmc load` error 0x80070005 | not elevated | use an Administrator shell |
| Lab client gets "connect" error | Driver not loaded, access denied, or port busy | Check `fltmc filters` and the lab account; use the [isolated probe](../docs/LIVE-COLLECTION.md), not the desktop |

## Altitude
`385201` is a development placeholder inside the Activity Monitor range (360000–389999). Request a Microsoft-assigned altitude and update `EXOR_ALTITUDE` and `exordrv.inf` before distributing.

## Signing
Production distribution requires Microsoft signing and validation on the supported security configuration. Do not disable Secure Boot or Memory Integrity on end-user systems. The disposable lab is not a way to enable live collection in the desktop.
