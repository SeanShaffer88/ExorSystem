# Audit and verification matrix

**Automated** means a test, build, or static package inspection ran; it does not imply an interactive Windows 10/11 pass. **Partial** means a native or manual gate remains. **Not run** means no evidence is available for that gate.

| Area | Status | Evidence and limits |
|---|---|---|
| Storage, schema v3 migration and append-only ledger | Automated | Linux/Windows tests cover migration backups, preservation of prior evidence, DAG constraints, case-aware lookup, transaction rollback and scoped ledger pages. |
| Engine, policy and collectors | Automated | Workspace fmt, strict Clippy and Rust tests on Linux and Windows CI. Registry/service/task observations cannot authorize file quarantine. |
| Quarantine record/ledger failures | Automated | `quarantine_transaction.rs` tests both policy and leftover entry points against record and second-ledger-row failures. Originals return and no partial database evidence commits. An interrupted restore can retry its ledger write without moving files twice. |
| Quarantine containment and recovery | Automated (simulated) | Tests reject invalid IDs, ignored database paths, redirected storage/batches and blob traversal. No-overwrite, corruption, conflicted originals, and both sides of the staged DB commit are exercised. Windows runner cases include real junctions and sharing denial; Linux exercises a cross-filesystem round trip. Real Windows shutdown/disk failure and cross-volume NTFS remain open. |
| Install Watch and session-scoped History | Partial | Mock snapshots and storage tests cover failures, deletions, cursor stability, and interleaved sessions. Built-bundle Chromium smoke covers Watch→History, filters, paging and retry using mocked IPC. A Windows runner reads existing/missing registry scopes without changing the OS. The real Windows 10/11 desktop flow has not run. |
| Classic/MSIX/portable inventory and guided uninstall | Partial | Engine and mocked browser checks cover source restrictions, per-user refusal, disappearing registrations, review-only system matches and portable entry lifecycle. Windows token/resolver tests read the current token and locate the system MSI executable without launching it. Actual GUI and uninstaller behavior remain open. |
| Desktop privilege and storage | Partial | `asInvoker`/PerMonitorV2 manifest and an early elevated-token refusal. Native known-folder profile has a single-instance lock; no old portable database is automatically trusted or imported. Check UAC, profile permissions, lock, migration behavior and launch on both client versions. |
| Frontend, themes and Tauri IPC | Partial | TypeScript/Vite build, all 36 theme contrast checks, and Chromium smoke with IPC stubs. Native commands, channel streaming, screen-reader behavior and presentation remain end-to-end gates. |
| Package scripts and publishing | Partial | [v0.1.0 packaging run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44039918347) published historical NSIS, MSI, portable ZIP and hashes. Build-only [candidate workflow](.github/workflows/windows-candidate.yml) verifies current hashes, package metadata and manifest on Windows Server. Interactive package install/upgrade/uninstall/UAC/WebView2 remain open. |
| Dependency licenses | Automated | Free/open-source license check in CI for the Rust workspaces and npm lockfile. |
| USN and registry driver parsers | Automated | Parser fixtures, malformed frames, and Windows FILETIME-to-Unix conversion. See [live-source evaluation](docs/LIVE-COLLECTION.md). |
| Live USN/registry port, kernel driver, WDK, signing | Not run | Windows user-mode sources and standalone lab probe are built/type-checked; real capture, driver build/load, assigned altitude and production signing have no evidence. Nothing is enabled or packaged by default. |
| Windows 10/11 accessibility, DPI and log-window docking | Not run | Empty [client evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md); no Narrator, high-contrast, or multi-monitor sign-off. |

The cleanup, desktop shell and live parser checks passed on [Linux/Windows CI](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44247178154). Candidate package artifacts and their exact commit/hash evidence are tracked separately in the PR's packaging check.

## Limits that affect release decisions

- The desktop never requests elevation. Windows Settings handles per-user uninstall and commands that cannot run safely without elevation. Some system files, tasks, services or registry scopes will be inaccessible to a normal account; incomplete probes are not evidence of absence.
- Quarantine is recoverable file handling, not a complete backup. No-overwrite applies to restore and rollback. Batch paths are checked again before filesystem work; these path checks and the profile lock do not protect against arbitrary modification by another process running as the same Windows user. Moves require a filesystem with hard links; the cross-volume copy verifies the default data stream. Conflicts and corrupt entries keep their held files for review. Real power loss and Windows cross-volume/long-path behavior are still unverified.
- The old executable-adjacent `data` directory can contain data from elevated or untrusted locations. It is left untouched. Restoring or importing its quarantine is not part of this version. Schema migrations still apply to databases in the current profile.
- Registry review is limited to Software key names. Services and tasks need an executable under the reported install folder to match by path. None of those findings authorizes changing the system item. MSIX inventory is current-user-only; portable inventory is explicit. Provenance's ASCII `NOCASE` fallback does not merge case-only paths.
- Registry snapshots are read-only, bounded, non-atomic and cannot attribute to a process. An inaccessible or incomplete scope is skipped. Optional USN and driver sources are separate, experimental and have stream-loss and shutdown gaps.

## Native sign-off

Use the [Windows runbook](docs/WINDOWS-RELEASE.md) and attach one [evidence record](docs/WINDOWS-RELEASE-EVIDENCE.md) for the exact candidate, naming each OS build and package hash. Verify actual Windows 10/11 startup, installation/upgrade/removal, Watch and History, cleanup drills, and accessibility. Keep [driver WDK/load, altitude and signing](docs/LIVE-COLLECTION.md#distribution-gates-open) separate.
