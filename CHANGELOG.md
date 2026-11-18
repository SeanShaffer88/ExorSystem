# Changelog

All notable changes. Format: Keep a Changelog; versions follow SemVer.

## [0.2.0]

See the [0.2.0 release notes](docs/releases/v0.2.0.md), including the required upgrade backup.

### Added
- A build-only Windows candidate workflow verifies hashes and actual NSIS/MSI/portable package metadata. A disposable file/HKCU fixture, native Windows 10/11 runbook, and evidence template document the separate interactive checks.
- An isolated, read-only lab probe and promotion criteria for experimental USN and driver sources; the driverless desktop is unchanged by these probes.
- Watch an install can compare explicitly selected `HKCU`, `HKLM` and `HKLM32` Software subtrees without the registry driver. It records key-level changes as read-only snapshot evidence and warns when a subtree could not be compared.
- Installations lists current-user MSIX/AppX packages alongside classic entries. Settings can add and forget opt-in portable inventory entries without running or removing their files.
- Find leftovers also suggests registry keys, services and scheduled tasks for classic programs, strictly for review. Recently removed entries stay available for review until ExorSystem closes. File quarantine requires explicit selection from eligible folders.
- Review classic uninstall leftovers and explicitly choose eligible file folders for quarantine; system matches cannot be removed here.
- Browser smoke test of the built frontend, with native IPC mocked, runs in GUI CI.
- Settings tab: pick the cleanup policy (conservative, standard, thorough) used by every preview, quarantine and report; shows version and data folders. Commands `list_templates`, `app_info`.
- Update risk tab: recorded programs batched low/medium/high with a plain-language reason; `risk::candidates_from_store`, `risk::explain`, command `update_risk`.
- Make portable copy: copies a recorded installation's files into a new folder with `portable-plan.json` (registry keys to replay, unsupported items); sources untouched. `portable::build_portable`, command `make_portable`.
- Save report: writes a plain-language Markdown report of a recorded installation to Documents (never overwrites); `report::session_report`, command `export_report`.
- Completed watches open History for their session. History labels evidence sources, includes recorded removals, and lets users page through changes with session and path filters.
- Installed programs list: `exor-engine::programs` (tidy, dedupe, match to recorded installs) and `exor-win::enumerate_programs` (HKLM 64/32-bit and HKCU Uninstall keys). Installations view is now three panes: program list, details with traced items, actions (preview cleanup, quarantine with confirm). Commands `list_programs`, `session_items`.
- Tabs across the top replace the side menu (arrow keys, Home/End, Alt+number). Status bar at the bottom shows the latest activity, with buttons for Watch, Scan, Quarantine, Themes and Log (Ctrl+`).
- Activity log window: small, fixed size, borderless, docked to the main window's right edge. It slides out from behind the main window and back; cannot be resized; follows the main window; follows the theme; honors reduced motion. Commands `get_log`, `clear_log`, `set_log_open`, `log_is_open`; engine activity is logged in plain language.
- CI license gate: `scripts/check-licenses.py` fails on any non-free dependency.
- Failure drills as tests: database record/ledger writes, no-overwrite, cross-filesystem quarantine, vanished file, reappeared file, deleted parent folder, interrupted restore, corrupt database, concurrent writers, upgrade backup usable as a rollback.

### Changed
- The Windows desktop now runs without elevation and refuses an already-elevated token. All formats use the current user's `%LOCALAPPDATA%\ExorSystem` profile. Old executable-adjacent data and quarantine are left unchanged, not migrated automatically.
- Only supported live machine-wide classic registrations can launch uninstall commands; per-user installs use Windows Settings. Restore never overwrites an existing file.
- The stable Release workflow publishes the exact signed-off candidate packages without rebuilding or replacing a tag.

### Fixed
- Both cleanup entry points persist the quarantine record and ledger atomically, roll back a failed write, and reconcile interrupted batches. Restore/purge derive storage paths by validated ID and refuse redirected directories. Conflicts retain held files with a warning.
- Switching tabs no longer loses the active Watch controls or its result. Failed Finish returns to a fresh baseline. System items stay out of file quarantine even under the Thorough policy.
- Provenance resolves unambiguous ASCII case variants without merging older case-only records. The v3 migration adds lookup indexes and preserves existing evidence.
- Layout: Appearance radio buttons no longer stretched; Watch an install form is stacked and its textarea styled (found in the first real Windows screenshots).

## [0.1.0] - 2026-11-17

Initial Windows x64 release: NSIS, MSI, and portable ZIP.

### Added
- Release workflow can also be started from the Actions tab (`workflow_dispatch`); it creates the tag itself.
- Install watch (`exor-engine::watch`): baseline, run the installer, compare. Folder snapshots plus services and scheduled tasks are recorded in one session. A failed probe skips that comparison with a warning instead of reporting everything as new. GUI: "Watch an install" view; Tauri commands `begin_watch`, `finish_watch`, `cancel_watch`.
- Failure drills as tests: disk full (ENOSPC), cross-filesystem quarantine, vanished file, reappeared file, deleted parent folder, interrupted restore, corrupt database, concurrent writers, upgrade backup usable as a rollback.
### Fixed
- An interrupted restore can now be retried: items already put back are recognised by content hash and skipped, instead of failing with "already exists".
- Optional themes: 36 palettes (dark and light) in `gui/src/themes.json`, Appearance view with live preview on selection, Apply / Cancel preview, and a temporary 30 s Undo after applying. Choice persists; the default look is unchanged. `npm run check:themes` enforces WCAG AA contrast and runs in `npm run build` and CI.
- Service and scheduled-task collectors: snapshot diffing, task XML parsing (UTF-8/UTF-16), SCM enumeration in `exor-win`, attribution into the DAG (heuristic 0.3, `depends_on` link to the executable when known).
- Quarantine execution (`exor-engine::execute`): quarantine what a policy decided, restore, purge, all recorded in the append-only ledger.
- Crash recovery: interrupted quarantines are rolled back at startup (`recover`, `find_incomplete`, status `rolled_back`).
- Quarantine view in the GUI (two-step confirm for move and permanent delete); Tauri commands `quarantine_session`, `list_quarantines`, `restore_quarantine`, `purge_quarantine`.
- GitHub Actions: `ci.yml` (fmt, clippy `-D warnings`, tests on Linux and Windows, GUI build) and tag-triggered `release.yml`.
- `scripts/release.ps1`: `ExorSystem-v{VERSION}-x64-Setup.exe`, `.msi`, `-Portable.zip`, `SHA256SUMS.txt`.
- App manifest requesting Administrator, PerMonitorV2 DPI, Win10/11 compatibility.
- Tauri v2 IPC channel streaming: `scan`/`cancel_scan` commands and Scan folder view with live progress; engine `capture_with_progress`.
- Driver test-signing loader `driver/scripts/dev-load.ps1` and troubleshooting table.
- Toolchain pinning: `rust-toolchain.toml` (1.97.0, MSVC target), static CRT config, `docs/DEVELOPMENT.md`.
- Storage layer (`exor-storage`): blueprint SQLite schema in WAL mode, transactional migrations with pre-upgrade backup, handlers for sessions, nodes, DAG edges (cycle-checked), policy rules, append-only change ledger, quarantine records, provenance, shared-component impact, orphaned-dependency review.
- Engine (`exor-engine`): tokio parallel walker, Merkle path-trie differential diff, BLAKE3 snapshots with incremental reuse, noise filter, mock USN/registry event sources, JSONLogic policy simulator and templates, manifest quarantine (restore, purge), installer fingerprinting, session monitor with the 0.3/0.7/1.0 confidence ladder, update risk batching, portable plan, plain-language reports.
- Windows sources (`exor-win`): USN journal reader, registry-driver client, portable and tested USN/driver-message parsers.
- Driver (`driver/`): minifilter-framework registry-callback driver, INF, wire-format header.
- GUI (`gui/`): Tauri v2 + React shell with Installations, Provenance and Cleanup simulator views; quarantine view behind a feature flag; window size/position persistence.
- Docs: README, ARCHITECTURE, AUDIT, CREDITS, DESCRIPTION, SCREENSHOTS capture list.

[0.1.0]: https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.1.0
