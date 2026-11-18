# Development environment

## Toolchain (Windows)
| Need | Detail |
|---|---|
| Rust | `rust-toolchain.toml` pins 1.97.0, target `x86_64-pc-windows-msvc`, static CRT via `.cargo/config.toml` |
| Visual Studio Build Tools 2022 | workload **Desktop development with C++** (MSVC v143 x64/x86 tools) |
| Windows SDK | 10.0.26100 or newer (for headers, `mt.exe` package checks and optional `signtool`) |
| WDK | the Windows Driver Kit matching the installed SDK build; only needed for `driver/` |
| WebView2 runtime | present on Windows 11; installer fallback on Windows 10 |
| Node | 22 LTS for `gui/` (`npm ci`) |

CI-style checks on any OS: `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.

## Frontend smoke test
From `gui/`, run `npm ci`, `npm run build`, `npx playwright-core install --only-shell chromium`, then `npm run test:ui`. Linux may need Chromium system libraries (`npx playwright-core install-deps chromium`). The test loads the built UI with mocked Tauri responses and checks the dashboard, read-only command preview, uninstall and quarantine confirmations, cleanup totals, registry watch, saved comparison coverage (including unknown, skipped, and zero-change states), diff details, history paging, and relationship navigation. It does not launch a real uninstaller or access the Windows registry. Rust tests exercise the watch with mock snapshots, and the Windows-only registry probe test reads standard key metadata without changing the registry. For optional end-to-end native checks, use the [Windows client checklist](../ROADMAP.md#optional-windows-client-testing).

To explore the [dashboard](DASHBOARD.md) without a Windows host, use `npm run dev` and open Vite in the browser. Only that development page without Tauri IPC uses the labeled, in-memory demo fixture.

## Windows privileges and profile
The `asInvoker` desktop uses the normal user token and refuses an elevated startup before opening storage. Run `npm run tauri dev` from a **non-elevated** terminal. A separate already-open desktop using the same `%LOCALAPPDATA%\ExorSystem` profile must be closed first. Engine/storage tests do not require elevation, and the normal watch does not need a driver.

## Build-only packages
See the [Windows release runbook](WINDOWS-RELEASE.md) for `scripts/release.ps1`, package checks, the disposable fixture, and interactive evidence. Use **Windows candidate packages (no publish)** for test artifacts. The separate **Release** workflow [tags and promotes the retained artifact](WINDOWS-RELEASE.md#6-tag-and-publish-the-tested-candidate) after CI and package verification, without requiring interactive client results.

## Optional live-source lab
See [the isolated lab runbook](LIVE-COLLECTION.md) and [driver development](../driver/README.md). Do not launch the desktop as Administrator to connect to a collector.

## IPC and the UI thread
Long work (`scan`) runs on Tauri's async runtime and streams `ScanEvent`s through a Tauri v2 `Channel`; progress is throttled to 100 ms so the webview never floods. `cancel_scan` stops the walk and hashing cooperatively.
