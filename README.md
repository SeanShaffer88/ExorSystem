# ExorSystem

Open-source Windows system management and installation governance suite: a FOSS Total Uninstall replacement built as an intelligent **System State & Provenance Engine**.

ExorSystem records observed system changes in a directed graph with confidence scores, so you can ask where an item came from and review the evidence before deciding what to remove.

## Layers

| Layer | Path | Tech | Status |
|---|---|---|---|
| GUI | `gui/` | Tauri v2 + React/TypeScript, keyboard-first, plain-language reports | tabs, three-pane programs list, status bar, slide-out log; type-checked |
| User-space engine | `crates/exor-engine` | Rust + tokio, BLAKE3 | implemented, 100% safe Rust, tested |
| Storage | `crates/exor-storage` | SQLite, WAL | implemented, tested |
| Windows sources | `crates/exor-win` | windows-rs: registry snapshots, inventory, USN journal, driver client | see [verification matrix](AUDIT.md) |
| Kernel driver | `driver/` | C, minifilter framework, `CmRegisterCallbackEx` | source only, not compiled |

See [architecture](docs/ARCHITECTURE.md), the [roadmap and release checks](docs/ROADMAP.md), the [verification matrix](AUDIT.md), and [changelog](CHANGELOG.md).

## Features

- **Program inventory**: classic Windows programs, current-user MSIX apps, and portable apps added explicitly in Settings. MSIX and portable entries are listed without uninstall actions.
- **Guided leftover review**: review possible folders, registry keys, services and tasks after a classic uninstall. System entries are read-only; only selected files from eligible folders can go to quarantine.
- **BOM & inference graph**: confidence `1.0` manifest-confirmed, `0.7` inferred from timing, `0.3` heuristic/unknown. Edges are cycle-checked.
- **Noise filtering**: background telemetry, browser caches, indexer and Defender writes dropped in real time, with burst de-duplication.
- **Differential engine**: async parallel directory walk, Merkle path-trie diff (identical subtrees skipped), BLAKE3 hashing with incremental reuse.
- **Install watch**: take a before picture, run the installer, then take an after picture. Record changes to files, services, scheduled tasks and optionally [specific registry subtrees](docs/ROADMAP.md#watch-an-install). Registry snapshots are read-only and need no driver.
- **Optional themes**: 36 dark and light palettes with live preview and a temporary Undo; all pass WCAG AA contrast.
- **Cleanup policy simulator**: JSONLogic rules, pure dry run; most protective rule wins (keep, then review, then quarantine). Templates: conservative, standard, thorough.
- **Evidence-preserving quarantine**: compact JSON manifests with BLAKE3 verification, all-or-nothing moves, restore and purge, manifests kept as evidence. No VSS snapshots.
- **Provenance timeline & change ledger**: append-only ledger (enforced by triggers); trace any artifact to its session, installer family and timestamp.
- **Governance extras**: shared-component impact maps, orphaned-dependency review, installer family fingerprinting (silent switches), update batching by risk, portable-app transition plan, accessibility-first summaries.
- **Mock harnesses**: seeded USN/registry event generators so the whole pipeline runs under `cargo test` without a driver.

## Build and test

```
cargo test --locked                     # engine + storage + parsers (any OS)
cargo clippy --all-targets --locked -- -D warnings
cd gui && npm ci && npm run build       # frontend
cd gui && npm run tauri build           # desktop app (Windows, WebView2)
```

See [development](docs/DEVELOPMENT.md) for the pinned Rust/Node toolchain and browser smoke checks. The optional driver needs the WDK; see [driver development](driver/README.md).

## Data

Everything is local and offline. The desktop app stores `data/exor.db` next to the executable. No telemetry.

## License

MIT. See `LICENSE`, `CREDITS.md`.
