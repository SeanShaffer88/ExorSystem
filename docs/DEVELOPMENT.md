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

## Frontend smoke test
From `gui/`, run `npm ci`, `npm run build`, `npx playwright-core install --only-shell chromium`, then `npm run test:ui`. Linux may need Chromium system libraries (`npx playwright-core install-deps chromium`). The test loads the built UI with mocked Tauri responses; it includes watch navigation, warnings, a failed Finish, and opening and paging the watch’s History, but it does not launch a real uninstaller or access the Windows registry. Rust tests exercise the watch with mock snapshots, and the Windows-only registry probe test reads standard key metadata without changing the registry. For end-to-end native checks, use [the Windows release checklist](../ROADMAP.md#before-publishing-a-windows-build).

## Administrator rights
The desktop app's manifest is `requireAdministrator`: the driver port, USN journal, services and HKLM need it. `npm run tauri dev` therefore must be started from an **elevated** terminal, or Windows refuses to launch the debug build. Engine and storage tests need no elevation.

## Kernel driver loop
See `driver/README.md`: use a disposable VM, `driver/scripts/dev-load.ps1`, test signing on.

## IPC and the UI thread
Long work (`scan`) runs on Tauri's async runtime and streams `ScanEvent`s through a Tauri v2 `Channel`; progress is throttled to 100 ms so the webview never floods. `cancel_scan` stops the walk and hashing cooperatively.
