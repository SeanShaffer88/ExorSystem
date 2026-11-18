# Audit and verification matrix

Labels: **Verified** (run here), **Partially verified**, **Unverified**.

| Area | Status | Evidence |
|---|---|---|
| Storage (schema, migrations, DAG, ledger, provenance) | Verified | 18 integration tests, Linux |
| Engine (walker, trie diff, BLAKE3, noise, policy, quarantine, monitor, collectors, execute) | Verified | 43 tests, Linux, `clippy -D warnings` |
| Install watch (files, services, tasks, optional registry subtrees) | Partially verified | Engine tests cover missing keys and failed sample pairs with mocks. Browser smoke covers warnings, tab navigation, Finish failure, and Cancel. A Windows-only test checks read-only traversal of an existing subtree and a missing scope; interactive Windows watch and installer correlation remain unverified. |
| Failure drills on Linux (ENOSPC, cross-filesystem, corrupt DB, concurrent writers, partial restore) | Verified | tests in `quarantine.rs` and `storage.rs` |
| Quarantine crash recovery | Verified (simulated) | crash state reproduced in a test; real power loss never exercised |
| Mock event harness | Verified | deterministic replay tests |
| USN/driver message parsers | Verified | unit tests incl. malformed input |
| USN journal reader, driver client (Win32 I/O) | Partially verified | type-checked and clippy-clean against the Windows target; never run on Windows |
| Dependency licenses (Rust + npm, 588) | Verified | all permissive or MPL-2.0; script enforced in CI |
| Service enumeration (SCM) | Partially verified | type-checked for the Windows target; never run on Windows |
| Release script, CI and release workflows | Partially verified | [v0.1.0 packaging run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44039918347) published NSIS, MSI, portable ZIP and hashes. [Base CI](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44047994722) passed on Linux and Windows; current-branch Windows checks pending. |
| Quarantine GUI view and Tauri commands | Partially verified | `tsc` + `vite build` pass, shell type-checked; never launched |
| Themes (36 palettes, preview, apply, undo) | Partially verified | WCAG AA contrast script passes; store logic smoke-tested with stubs; never rendered or screen-reader tested |
| Kernel driver `driver/exordrv.c`, INF | Unverified | not compiled with the WDK, not loaded |
| GUI frontend | Partially verified | `tsc`, `vite build` and a Chromium smoke test with mocked IPC pass; not screen-reader tested |
| GUI Tauri shell | Partially verified | type-checked for the Windows target; never launched |
| Administrator manifest (`requireAdministrator`, PMv2 DPI) | Partially verified | manifest embedded via tauri-build, type-checked; UAC prompt never observed |
| IPC streaming (`scan` + `cancel_scan` over Tauri channel) | Partially verified | engine progress API tested (monotonic, ends `done`); channel path type-checked only |
| Installed programs enumeration (`exor-win`) | Partially verified | Classic entries plus main MSIX packages for the calling user. Browser smoke uses fixtures; new WinRT package enumeration needs a Windows runtime check. |
| Portable inventory and path lookups (schema v3) | Partially verified | Migration, case-only collisions and portable upsert/removal checked on SQLite. Portable registration and case lookup regression tests added for Cargo CI. |
| Guided uninstall and leftover review | Partially verified | Browser fixture covers a disappearing uninstall entry, explicit folder selection, and read-only registry/service/task findings. Live review probes and uninstaller behavior still need Windows validation. |
| History, report export, portable copy, update risk, settings | Partially verified | engine and storage parts unit-tested; GUI built and type-checked; never rendered on Windows; Documents-folder output paths untested on real profiles |
| Log window slide, z-order, docking | Unverified | type-checked only; behavior on real Windows (animation, staying behind main, multi-monitor, DPI) never observed |
| `driver/scripts/dev-load.ps1` | Unverified | never run |
| Windows 10 vs 11 behaviour, DPI, high contrast | Unverified | |

## Known limitations
- A release-signed kernel driver needs a code-signing certificate, which is not free. Test-signing works for development; source stays open either way.
- Theme palettes are inspired by well-known schemes. Colors were nudged where needed to meet WCAG AA, so they are not exact upstream values. Three names were changed to avoid third-party brand names (see CREDITS).
- The driver reports `RegNtPreDeleteKey` as an attempt.
- The registry client thread exits at the next message after the consumer drops (blocking `FilterGetMessage`).
- Dev altitude `385201` is a placeholder; Microsoft must assign a real one.
- Registry review checks key names under Software and one exact publisher key, not values or the whole registry. Service/task matching needs a command with an absolute local path under the reported install folder. Inaccessible entries and unsupported command formats can be missed. System entries are never changed.
- MSIX inventory is current-user-only and read-only. Portable entries are opt-in rather than auto-detected.
- Trace lookup uses SQLite ASCII `NOCASE` as a fallback; node identity and write-side matching remain exact to preserve case-only records.
- The optional live USN and driver sources are separate library components. Desktop Watch uses file, service, task, and opt-in registry snapshots. Registry snapshots are bounded and non-atomic; only key metadata is compared, not value data or live events. An inaccessible or incomplete scope is not compared.

## Failure drills not yet run
See [Windows release checks](docs/ROADMAP.md). Need Windows or a person: locked files (sharing violations), cross-volume moves on NTFS, antivirus interference, power loss during a real migration or quarantine, Windows long paths and reparse points. Run as root here, so permission-denied cases were not exercised.
