# Changelog

All notable changes. Format: Keep a Changelog; versions follow SemVer.

## [Unreleased] - 0.1.0 in progress
### Added
- Guided uninstall: Uninstall runs the program's own registered uninstaller (no shell; command read from the registry, never from the UI), then Find leftovers suggests folders (recorded install folder 0.7, name match 0.3; publisher folders never match) and quarantines the chosen ones, reversibly. `exor-engine::leftovers`; commands `run_uninstaller`, `find_leftovers`, `quarantine_leftovers` (only suggested folders are accepted; roots and `\\Windows` refused).
- Installed programs list: `exor-engine::programs` (tidy, dedupe, match to recorded installs) and `exor-win::enumerate_programs` (HKLM 64/32-bit and HKCU Uninstall keys). Installations view is now three panes: program list, details with traced items, actions (preview cleanup, quarantine with confirm). Commands `list_programs`, `session_items`.
- Tabs across the top replace the side menu (arrow keys, Home/End, Alt+number). Status bar at the bottom shows the latest activity, with buttons for Watch, Scan, Quarantine, Themes and Log (Ctrl+`).
- Activity log window: small, fixed size, borderless, docked to the main window's right edge. It slides out from behind the main window and back; cannot be resized; follows the main window; follows the theme; honors reduced motion. Commands `get_log`, `clear_log`, `set_log_open`, `log_is_open`; engine activity is logged in plain language.
- CI license gate: `scripts/check-licenses.py` fails on any non-free dependency.
- Release workflow can also be started from the Actions tab (`workflow_dispatch`); it creates the tag itself.
- Install watch (`exor-engine::watch`): baseline, run the installer, compare. Folder snapshots plus services and scheduled tasks are recorded in one session. A failed probe skips that comparison with a warning instead of reporting everything as new. GUI: "Watch an install" view; Tauri commands `begin_watch`, `finish_watch`, `cancel_watch`.
- Failure drills as tests: disk full (ENOSPC), cross-filesystem quarantine, vanished file, reappeared file, deleted parent folder, interrupted restore, corrupt database, concurrent writers, upgrade backup usable as a rollback.
### Fixed
- Layout: Appearance radio buttons no longer stretched; Watch an install form is stacked and its textarea styled (found in the first real Windows screenshots).
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
