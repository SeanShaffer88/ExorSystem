# Credits

- Concept and blueprint: Sean (ProTechSean / SeanShaffer88).
- Initial implementation: Claude (Anthropic) via Claude Code; see the Git history for subsequent contributions.
- Libraries (MIT/Apache-2.0 unless noted): tokio, rusqlite (bundled SQLite, public domain), blake3 (CC0/Apache-2.0), serde, serde_json, thiserror, windows-rs, tempfile, Tauri, React, Vite, TypeScript.
- Format reference: JSONLogic (https://jsonlogic.com), reimplemented as a subset in `jsonlogic.rs`; no code copied.
- Microsoft documentation for USN journal, `CmRegisterCallbackEx` and minifilter communication ports.

## Theme inspiration
Palettes are original approximations inspired by: Catppuccin, Nord, Tokyo Night, Dracula, Gruvbox, Everforest, Rosé Pine, Kanagawa, Aura, Solarized, Monokai, shadcn/ui, KDE Breeze, QDarkStyleSheet, egui_sauge, egui-thematic, TintKit, Material, Windows Fluent. No upstream theme files are bundled. "Midnight Shield" and "Aged Parchment" are generic names for the requested styles.

## Licensing
ExorSystem is licensed under [MIT](LICENSE). The [CI license check](scripts/check-licenses.py) evaluates allowed SPDX expressions from both locked Rust workspaces and the npm lockfile. The current result comes from CI; dependency counts can change with the lockfiles.

MSVC and the Windows SDK are external build tools; WebView2 Runtime is a Windows desktop prerequisite licensed separately. The WDK is for the [experimental driver lab](docs/LIVE-COLLECTION.md), not the normal desktop build.
