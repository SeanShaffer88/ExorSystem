# Audit and verification matrix

This page separates automated results for the current desktop from native client and driver-lab results. **Automated** means a test, build, or static inspection ran. **Partial** means the area has some evidence but open scenarios. **Not run** means no result is recorded for the named scenario. Neither Windows Server CI nor mocked browser tests count as Windows 10/11 client runs.

| Area | Status | Evidence and limits |
|---|---|---|
| Storage, migrations, and History | Automated | Storage tests cover migration backup, preservation of older ledger rows, DAG constraints, case-aware lookup, rollback, and paged/filterable history. Schema v5 adds cleanup links, batch indexes, and operation intent. Test the full upgrade with held files on a client separately. |
| Exact review and quarantine | Partial | Engine/Tauri tests cover bounded plans, stale identities, changed files, no-replace restore, and rollback after simulated ledger or manifest failures. Windows Server tests exercise sharing and reparse cases; the Windows moves are same-volume and handle-bound. These tests do not verify power-loss durability or the full native review workflow. |
| Profile archive and rotation | Partial | Tests cover byte verification, invalid indexes, changed or missing held files, and separate publication. Archives are not system images. Client backup/rollback, native ACL/stream behavior, and fault drills are separate. |
| Driverless Watch and History | Partial | Mock snapshots and storage tests exercise skipped scopes, removals, cancellation, and atomic saved comparisons. Windows tests read bounded registry metadata. The built UI is tested with mocked IPC, including Watch → History, dated filters, and export continuation. An earlier file-only PC smoke is partial evidence for that run, not a v0.3.0 client pass. |
| Inventory and uninstall review | Partial | Automated tests cover supported-registration checks, per-user refusal, advisory findings, and portable-entry lifecycle. The real vendor uninstaller, Windows Settings fallback, and complete GUI review still need client tests. |
| Docking, themes, and native IPC | Partial | The frontend build checks 36 theme palettes for WCAG AA contrast. Chromium covers the built UI with mocked IPC. Windows Rust tests check window placement using temporary native windows, not the packaged WebView2. Native IPC allow/deny, Narrator, high contrast, and mixed-DPI behavior are **Not run**. |
| Candidate packaging and publication | Automated | CI, exact-artifact promotion, MSI and portable manifest checks, and the published-asset recheck are linked below. No installer or desktop ran in these package checks. |
| Interactive Windows 10/11 packages | Not run | Install/extract, launch and exit, profile/UAC, WebView2 render, upgrade/removal, and the end-to-end workflow matrix remain open. The optional launch observer has been parsed and refused on Windows Server; it has no client result. |
| Dependency licenses | Automated | CI checks allowed SPDX expressions in both Rust workspaces and the npm lockfile. See [credits](CREDITS.md). |
| Optional live-source parsers and driver | Partial | Parser fixtures, malformed frames, and timestamp conversion are covered. Native USN/callback capture, WDK build/load, altitude assignment, and production signing are **Not run**; neither live source powers desktop Watch. |

## v0.3.0 release evidence

[ExorSystem v0.3.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.3.0) was promoted from source `b34188983fdfe2d5e502380ce02873131d388c8e` without rebuilding. The [desktop remediation details](docs/AUDIT-REMEDIATION.md) explain v0.3.0's controls and remaining native checks.

| Check | Result and scope |
|---|---|
| [CI](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44415431971) and [Windows candidate](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44415431944) | Pass on the release source. Candidate checks inspect hashes, MSI summary/template, and portable x64 executable and manifest; they do not execute the packages. |
| [Promotion](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44432420633) | Pass. The three exact retained packages, `SHA256SUMS.txt`, and `PACKAGE-CHECKS.json` were published together. |
| [Published package verification](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44434816182) ([PR #33](https://github.com/SeanShaffer88/ExorSystem/pull/33)) | Pass. All five downloads matched GitHub's recorded sizes and digests. The package hashes and candidate version, commit, and run agreed with the checksum file and candidate report. The runner wrote `packages.json` and did not extract, install, or launch the desktop. |
| Windows 10/11 client matrix | **Not run.** Use the [Windows runbook](docs/WINDOWS-RELEASE.md) and fill the [client evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md) only for observations made on the named build, OS, and format. |

## v0.2.0 automated evidence

[Linux/Windows Rust and GUI/Tauri CI](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44272786809) and the [Windows Server candidate workflow](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44272786871) passed for `c3aa8c0c0c2d8ac20d107f62b91c38b5eb67b142`. The candidate workflow exercised the disposable fixture and inspected the NSIS, MSI, and portable ZIP. It did **not** install or launch those packages on Windows 10/11. The [promotion run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44284479160) published the retained packages without rebuilding. This is historical evidence for v0.2.0, not a validation of later code.

## Baseline limits that affect release decisions

- The desktop does not elevate. Use Windows Settings for per-user uninstall and unsupported commands. An inaccessible scope is not evidence of absence.
- Quarantine is recoverable file handling, not a full application backup. Moves do not fall back to copying across volumes. Conflicts and invalid blobs are retained for review as applicable; an interrupted purge never resumes on startup. Same-user software can still change local files outside the app's control.
- Back up the full profile before the v5 migration, not just the database. The legacy `data` folder beside the executable is left untouched; neither its records nor held files are imported.
- Snapshots are bounded, non-atomic, and not process-attributed. Registry comparison reads key paths and last-write metadata under chosen Software subtrees, not value contents. System findings cannot authorize file cleanup. Live USN and driver sources are separate lab work.

## Optional native Windows testing

Stable publishing requires CI and a verified exact candidate, not client sign-off. Use clean disposable Windows 10/11 x64 VMs and the [first-launch observer](docs/WINDOWS-RELEASE.md#8-observe-first-launch-and-exit-on-a-disposable-client) for minimal launch/exit evidence. Verify installer actions, UAC, WebView2, Watch, cleanup, and accessibility separately. Keep [WDK/load, altitude, and signing](docs/LIVE-COLLECTION.md#distribution-gates-open) in the lab matrix.
