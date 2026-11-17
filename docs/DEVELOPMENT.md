# Development environment

## Toolchain (Windows)
| Need | Detail |
|---|---|
| Rust | `rust-toolchain.toml` pins 1.97.0, target `x86_64-pc-windows-msvc`, static CRT via `.cargo/config.toml` |
| Visual Studio Build Tools 2022 | workload **Desktop development with C++** (MSVC v143 x64/x86 tools) |
| Windows SDK | 10.0.26100 or newer (for `signtool`, headers) |
| WDK | the Windows Driver Kit matching the installed SDK build; only needed for `driver/` |
| WebView2 runtime | present on Windows 11; installer fallback on Windows 10 |
| Node | 22 LTS for `gui/` (`npm ci`) |

CI-style checks on any OS: `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.

## Administrator rights
The desktop app's manifest is `requireAdministrator`: the driver port, USN journal, services and HKLM need it. `npm run tauri dev` therefore must be started from an **elevated** terminal, or Windows refuses to launch the debug build. Engine and storage tests need no elevation.

## Kernel driver loop
See `driver/README.md`: use a disposable VM, `driver/scripts/dev-load.ps1`, test signing on.

## IPC and the UI thread
Long work (`scan`) runs on Tauri's async runtime and streams `ScanEvent`s through a Tauri v2 `Channel`; progress is throttled to 100 ms so the webview never floods. `cancel_scan` stops the walk and hashing cooperatively.
