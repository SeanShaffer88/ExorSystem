# Changelog

All notable changes. Format: Keep a Changelog; versions follow SemVer.

## [Unreleased] - 0.1.0 in progress
### Added
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
