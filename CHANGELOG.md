# Changelog

All notable changes. Format: Keep a Changelog; versions follow SemVer.

## [Unreleased] - 0.1.0 in progress
### Added
- Storage layer (`exor-storage`): blueprint SQLite schema in WAL mode, transactional migrations with pre-upgrade backup, handlers for sessions, nodes, DAG edges (cycle-checked), policy rules, append-only change ledger, quarantine records, provenance, shared-component impact, orphaned-dependency review.
- Engine (`exor-engine`): tokio parallel walker, Merkle path-trie differential diff, BLAKE3 snapshots with incremental reuse, noise filter, mock USN/registry event sources, JSONLogic policy simulator and templates, manifest quarantine (restore, purge), installer fingerprinting, session monitor with the 0.3/0.7/1.0 confidence ladder, update risk batching, portable plan, plain-language reports.
- Windows sources (`exor-win`): USN journal reader, registry-driver client, portable and tested USN/driver-message parsers.
- Driver (`driver/`): minifilter-framework registry-callback driver, INF, wire-format header.
- GUI (`gui/`): Tauri v2 + React shell with Installations, Provenance and Cleanup simulator views; quarantine view behind a feature flag; window size/position persistence.
- Docs: README, ARCHITECTURE, AUDIT, CREDITS, DESCRIPTION, SCREENSHOTS capture list.
