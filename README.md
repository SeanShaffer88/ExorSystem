# ExorSystem

Open-source Windows system management and installation governance suite: a FOSS Total Uninstall replacement built as an intelligent **System State & Provenance Engine**.

ExorSystem records observed system changes in a directed graph with confidence scores, so you can ask where an item came from and review the evidence before deciding what to remove.

## Downloads

Get the Windows x64 NSIS installer, MSI, or portable ZIP from [GitHub Releases](https://github.com/SeanShaffer88/ExorSystem/releases). Compare the package's SHA-256 with the `SHA256SUMS.txt` attached to the same release before running it.

**Upgrading from v0.1.0:** The newer desktop runs without administrator rights and starts with a per-user profile. Back up your old executable-adjacent `data` folder, including any quarantine batches, before upgrading. The new version does not import or restore those batches automatically. See [Data](#data) and the [0.2.0 release notes](docs/releases/v0.2.0.md).

## Layers

| Layer | Path | Tech | Status |
|---|---|---|---|
| GUI | `gui/` | Tauri v2 + React/TypeScript, keyboard-first, plain-language reports | dockable workspace, split inspector, guided uninstall, activity log |
| User-space engine | `crates/exor-engine` | Rust + tokio, BLAKE3 | implemented, 100% safe Rust, tested |
| Storage | `crates/exor-storage` | SQLite, WAL | implemented, tested |
| Windows sources | `crates/exor-win` | windows-rs: registry snapshots, inventory, USN journal, driver client | see [verification matrix](AUDIT.md) |
| Kernel driver | `driver/` | C, minifilter framework, `CmRegisterCallbackEx` | source only, not compiled |

See the [dashboard guide](docs/DASHBOARD.md), [architecture](docs/ARCHITECTURE.md), the [roadmap and release checks](ROADMAP.md), the [verification matrix](AUDIT.md), and [changelog](CHANGELOG.md).

## Features

- **Dockable workspace**: arrange tools as tabs in Main, Right or Bottom. Resize splits, apply a layout preset, or keep Watch and the Activity log beside the evidence you are reviewing. Overview shows inventory, recent changes, and cleanup metrics. Storage totals reflect recorded file sizes, not measured free space.
- **Program inventory**: classic Windows programs, current-user MSIX apps, and portable apps added explicitly in Settings. MSIX and portable entries are listed without uninstall actions.
- **Guided leftover review**: preview the registered command, confirm the vendor uninstall, and separately review possible leftover folders, registry keys, services and tasks. ExorSystem runs without elevation and launches only supported live machine-wide registrations; use Windows Settings for per-user or incompatible uninstallers. System findings are read-only.
- **BOM & inference graph**: confidence `1.0` manifest-confirmed, `0.7` inferred from timing, `0.3` heuristic/unknown. Edges are cycle-checked.
- **Noise filtering**: background telemetry, browser caches, indexer and Defender writes dropped in real time, with burst de-duplication.
- **Differential engine**: async parallel directory walk, Merkle path-trie diff (identical subtrees skipped), BLAKE3 hashing with incremental reuse.
- **Install watch**: take a before picture, run the installer, then take an after picture. Compare selected folders and [specific registry subtrees](ROADMAP.md#watch-an-install), with separately opt-in service and scheduled-task checks. See which scopes were compared, skipped, or not requested. Registry snapshots are read-only and need no driver.
- **Optional themes**: 36 dark and light palettes with live preview and a temporary Undo; all pass WCAG AA contrast.
- **Cleanup policy simulator**: JSONLogic rules, pure dry run; most protective rule wins (keep, then review, then quarantine). Templates: conservative, standard, thorough.
- **Evidence-preserving quarantine**: BLAKE3-verified file batches, no-overwrite restore, explicit permanent delete, and manifests kept as evidence. Interrupted batches are recovered when safe; conflicts retain the held files. No VSS snapshots.
- **Provenance & change history**: trace recorded artifacts and review an installation's observations in the append-only ledger. The split change inspector shows before/after evidence; its timeline supports session and path filters, older pages, and evidence source labels. Trace recorded dependencies in the relationship graph.
- **Governance extras**: shared-component impact maps, orphaned-dependency review, installer family fingerprinting (silent switches), update batching by risk, portable-app transition plan, accessibility-first summaries.
- **Mock harnesses**: seeded USN/registry event generators so the whole pipeline runs under `cargo test` without a driver.

## Build and test

```
cargo test --locked                     # engine + storage + parsers (any OS)
cargo clippy --all-targets --locked -- -D warnings
cd gui && npm ci && npm run build       # frontend
cd gui && npm run tauri build           # desktop app (Windows, WebView2)
```

See [development](docs/DEVELOPMENT.md) for the pinned Rust/Node toolchain and browser smoke checks. See the [Windows release runbook](docs/WINDOWS-RELEASE.md) for build-only packages, stable promotion, and optional native testing. The [optional live sources](docs/LIVE-COLLECTION.md) are not used by the desktop.

## Data

Everything is local and offline. On Windows, all three package formats store the database and quarantine under `%LOCALAPPDATA%\ExorSystem` for the launching user. The desktop does not run as Administrator. Older `data` directories beside the executable are left untouched and not imported; Settings reports one if it is present. No telemetry.

## License

MIT. See `LICENSE`, `CREDITS.md`.
