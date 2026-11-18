# Audit and verification matrix

**Automated** means a test, build, or static package inspection ran; it does not imply an interactive Windows 10/11 pass. **Partial** means some scenarios have not been exercised. **Not run** means no test result is available for that area.

| Area | Status | Evidence and limits |
|---|---|---|
| Storage, migrations and append-only ledger | Automated | Tests cover migration backups, including v3 → v4 without inventing coverage, preservation of prior evidence, DAG constraints, case-aware lookup, transaction rollback and scoped ledger pages. |
| Engine, policy and collectors | Automated | Workspace fmt, strict Clippy and Rust tests on Linux and Windows CI. Registry/service/task observations cannot authorize file quarantine. |
| Quarantine record/ledger failures | Automated | `quarantine_transaction.rs` tests both policy and leftover entry points against record and second-ledger-row failures. Originals return and no partial database evidence commits. An interrupted restore can retry its ledger write without moving files twice. |
| Quarantine containment and recovery | Automated (simulated) | Tests reject invalid IDs, ignored database paths, redirected storage/batches and blob traversal. No-overwrite, corruption, conflicted originals, and both sides of the staged DB commit are exercised. Windows runner cases include real junctions and sharing denial; Linux exercises a cross-filesystem round trip. Real Windows shutdown/disk failure and cross-volume NTFS remain open. |
| Install Watch and session-scoped History | Partial | Mock snapshots and storage tests cover failures, deletions, cursor stability, interleaved sessions, and durable comparison coverage. Injected ledger and coverage write failures roll back the comparison. Built-bundle Chromium smoke covers Watch→History, saved coverage, filters, paging and retry using mocked IPC. A Windows runner reads existing/missing registry scopes without changing the OS. The full interactive Windows 10/11 matrix has not run on the v0.2.0 packages. |
| Classic/MSIX/portable inventory and guided uninstall | Partial | Engine and mocked browser checks cover source restrictions, per-user refusal, disappearing registrations, review-only system matches and portable entry lifecycle. Windows token/resolver tests read the current token and locate the system MSI executable without launching it. Actual GUI and uninstaller behavior remain open. |
| Desktop privilege and storage | Partial | `asInvoker`/PerMonitorV2 manifest and an early elevated-token refusal. Native known-folder profile has a single-instance lock; no old portable database is automatically trusted or imported. Check UAC, profile permissions, lock, migration behavior and launch on both client versions. |
| Frontend, themes and Tauri IPC | Partial | TypeScript/Vite build, all 36 theme contrast checks, and Chromium smoke with IPC stubs, including docking, layout persistence, retained Watch/Scan and activity log ordering. Native commands, channel streaming, screen-reader behavior and presentation remain to be checked end to end. |
| Package scripts and publishing | Partial | [v0.2.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.2.0) published NSIS, MSI, portable ZIP, and checksums from the exact retained candidate. Build-only [candidate workflow](.github/workflows/windows-candidate.yml) verifies current hashes, package metadata and manifest on Windows Server. Interactive package install/upgrade/uninstall/UAC/WebView2 remain open. |
| Dependency licenses | Automated | Free/open-source license check in CI for the Rust workspaces and npm lockfile. |
| USN and registry driver parsers | Automated | Parser fixtures, malformed frames, and Windows FILETIME-to-Unix conversion. See [live-source evaluation](docs/LIVE-COLLECTION.md). |
| Live USN/registry port, kernel driver, WDK, signing | Not run | Windows user-mode sources and standalone lab probe are built/type-checked; real capture, driver build/load, assigned altitude and production signing have no evidence. Nothing is enabled or packaged by default. |
| Windows 10/11 accessibility, DPI and log-window docking | Not run | Empty [client evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md); no Narrator, high-contrast, or multi-monitor test results. |

## Unreleased audit-remediation work

See [the remediation verification matrix](docs/AUDIT-REMEDIATION.md) for the new controls, local checks, and the still-open native release gates. The table above is historical baseline evidence.

## v0.2.0 automated evidence

[Linux/Windows Rust and GUI/Tauri CI](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44272786809) and the [Windows Server candidate workflow](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44272786871) passed for `c3aa8c0c0c2d8ac20d107f62b91c38b5eb67b142`. The candidate workflow exercised the disposable fixture and inspected the NSIS, MSI and portable ZIP, including their SHA-256 hashes and package metadata. It did **not** install or launch the packages on Windows 10/11. The [promotion run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44284479160) verified and published the retained packages without rebuilding. Driver verification remains separate.

## Baseline limits that affect release decisions

- The desktop never requests elevation. Windows Settings handles per-user uninstall and commands that cannot run safely without elevation. Some system files, tasks, services or registry scopes will be inaccessible to a normal account; incomplete probes are not evidence of absence.
- Quarantine is recoverable file handling, not a complete backup. No-overwrite applies to restore and rollback. Batch paths are checked again before filesystem work; these path checks and the profile lock do not protect against arbitrary modification by another process running as the same Windows user. Moves require a filesystem with hard links; the cross-volume copy verifies the default data stream. Conflicts and corrupt entries keep their held files for review. Real power loss and Windows cross-volume/long-path behavior are still unverified.
- The old executable-adjacent `data` directory can contain data from elevated or untrusted locations. It is left untouched. Restoring or importing its quarantine is not part of this version. Schema migrations still apply to databases in the current profile.
- Registry review is limited to Software key names. Services and tasks need an executable under the reported install folder to match by path. None of those findings authorizes changing the system item. MSIX inventory is current-user-only; portable inventory is explicit. Provenance's ASCII `NOCASE` fallback does not merge case-only paths.
- Registry snapshots are read-only, bounded, non-atomic and cannot attribute to a process. An inaccessible or incomplete scope is skipped. Optional USN and driver sources are separate, experimental and have stream-loss and shutdown gaps.

## Optional native Windows testing

Stable publishing currently requires CI and exact-artifact verification, not a native client sign-off. The [remediation matrix](docs/AUDIT-REMEDIATION.md) lists additional Windows checks to run for the new code. When running the optional tests, use the [Windows runbook](docs/WINDOWS-RELEASE.md) and attach an [evidence record](docs/WINDOWS-RELEASE-EVIDENCE.md) for the exact candidate, naming each OS build and package hash. Verify actual Windows 10/11 startup, installation/upgrade/removal, Watch and History, cleanup drills, and accessibility. Keep [driver WDK/load, altitude and signing](docs/LIVE-COLLECTION.md#distribution-gates-open) separate.
