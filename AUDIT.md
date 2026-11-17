# Audit and verification matrix

Labels: **Verified** (run here), **Partially verified**, **Unverified**.

| Area | Status | Evidence |
|---|---|---|
| Storage (schema, migrations, DAG, ledger, provenance) | Verified | 15 integration tests, Linux |
| Engine (walker, trie diff, BLAKE3, noise, policy, quarantine, monitor, collectors, execute) | Verified | 34 tests, Linux, `clippy -D warnings` |
| Quarantine crash recovery | Verified (simulated) | crash state reproduced in a test; real power loss never exercised |
| Mock event harness | Verified | deterministic replay tests |
| USN/driver message parsers | Verified | unit tests incl. malformed input |
| USN journal reader, driver client (Win32 I/O) | Partially verified | type-checked and clippy-clean against the Windows target; never run on Windows |
| Service enumeration (SCM) | Partially verified | type-checked for the Windows target; never run on Windows |
| Release script, CI and release workflows | Unverified | YAML parses; scripts and workflows never run (no Windows runner here) |
| Quarantine GUI view and Tauri commands | Partially verified | `tsc` + `vite build` pass, shell type-checked; never launched |
| Kernel driver `driver/exordrv.c`, INF | Unverified | not compiled with the WDK, not loaded |
| GUI frontend | Partially verified | `tsc` + `vite build` pass; not rendered or screen-reader tested |
| GUI Tauri shell | Partially verified | type-checked for the Windows target; never launched |
| Administrator manifest (`requireAdministrator`, PMv2 DPI) | Partially verified | manifest embedded via tauri-build, type-checked; UAC prompt never observed |
| IPC streaming (`scan` + `cancel_scan` over Tauri channel) | Partially verified | engine progress API tested (monotonic, ends `done`); channel path type-checked only |
| `driver/scripts/dev-load.ps1` | Unverified | never run |
| Windows 10 vs 11 behaviour, DPI, high contrast | Unverified | |

## Known limitations
- The driver reports `RegNtPreDeleteKey` as an attempt.
- The registry client thread exits at the next message after the consumer drops (blocking `FilterGetMessage`).
- Dev altitude `385201` is a placeholder; Microsoft must assign a real one.
- The live service enumerator exists, but the GUI has no screen that runs it yet; collectors feed the DAG through the engine API.
- Installers are produced only by `scripts/release.ps1` on a Windows machine; none have been built.
- Path matching in SQLite is case-sensitive (blueprint schema); the trie and ledger lookups are case-insensitive.

## Failure drills not yet run
Crash mid-quarantine on real volumes (the rollback logic itself is tested), disk full, locked files, cross-volume moves, power loss during migration.
