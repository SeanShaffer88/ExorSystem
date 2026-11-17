# Credits

- Concept and blueprint: Sean (ProTechSean / SeanShaffer88).
- Implementation: Claude (Anthropic) via Claude Code.
- Libraries (MIT/Apache-2.0 unless noted): tokio, rusqlite (bundled SQLite, public domain), blake3 (CC0/Apache-2.0), serde, serde_json, thiserror, windows-rs, tempfile, Tauri, React, Vite, TypeScript.
- Format reference: JSONLogic (https://jsonlogic.com), reimplemented as a subset in `jsonlogic.rs`; no code copied.
- Microsoft documentation for USN journal, `CmRegisterCallbackEx` and minifilter communication ports.

## Theme inspiration
Palettes are original approximations inspired by: Catppuccin, Nord, Tokyo Night, Dracula, Gruvbox, Everforest, Rosé Pine, Kanagawa, Aura, Solarized, Monokai, shadcn/ui, KDE Breeze, QDarkStyleSheet, egui_sauge, egui-thematic, TintKit, Material, Windows Fluent. No upstream theme files are bundled. "Midnight Shield" and "Aged Parchment" are generic names for the requested styles.

## Licensing
ExorSystem is free and open source (MIT). `scripts/check-licenses.py` runs in CI and fails if any Rust or npm dependency is not under a free/open-source license (permissive, or MPL-2.0). Current result: 588 dependencies, all allowed.
Build-time tools outside the repo are Microsoft SDKs (WDK, Windows SDK, WebView2 runtime). They are free of charge but not open source; they are not bundled.
