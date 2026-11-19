# Development environment

Use the [documentation index](README.md) for the desktop guide, release runbook, and verification status.

## Toolchain (Windows)

| Need | Detail |
|---|---|
| Rust | [`rust-toolchain.toml`](../rust-toolchain.toml) pins 1.97.0 and `x86_64-pc-windows-msvc`. MSVC uses static CRT via [`.cargo/config.toml`](../.cargo/config.toml). |
| Visual Studio Build Tools 2022 | **Desktop development with C++** and the MSVC v143 x64/x86 tools. |
| Windows SDK | 10.0.26100 or newer for headers and `mt.exe` package inspection. |
| WebView2 Runtime | Required to run the Windows desktop. Record the actual runtime version and install behavior for client evidence; CI build success does not prove either. |
| Node.js | Node 22 for `gui/`; use `npm ci` to install the locked dependencies. |
| Python and PowerShell | Python 3.11+ for license/asset checks. Use x64 PowerShell 7.2+ for the optional Windows client launch observer. |
| WDK | Only for the separate `driver/` lab, not the driverless desktop or desktop packaging. |

## Workspace checks

From the repository root, use the same fmt and locked gates as Linux/Windows CI:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/check-licenses.py
```

The root workspace covers engine, storage, filesystem primitives, and Windows sources. The Tauri shell has its own Cargo workspace; from `gui/src-tauri`, run `cargo fmt --check` and `cargo clippy --locked -- -D warnings` with the Windows toolchain. Windows-only collectors and native placement tests do not run on Linux.

## Run the desktop

On Windows, from a **non-elevated** terminal:

```powershell
cd gui
npm ci
npm run tauri dev
```

Close any other instance using the same `%LOCALAPPDATA%\ExorSystem` profile. Tauri dev uses the normal profile; use a disposable Windows account to avoid modifying real records. The `asInvoker` desktop refuses an elevated token before opening storage. Never elevate the desktop to connect to a collector.

## Frontend smoke test

From `gui/`:

```sh
npm ci
npm run build
npx playwright-core install --only-shell chromium
npm run test:ui
```

Linux may need Chromium system libraries (`npx playwright-core install-deps chromium`). The build type-checks the frontend and checks all 36 theme palettes for WCAG AA contrast. The four browser suites load the built UI with mocked Tauri IPC and exercise Watch/History, docking, cleanup approval, export, backup, and Activity log behavior. They do not run vendor uninstallers, use native IPC/WebView2, or write real app exports. Windows Rust placement tests use temporary native windows; they are not WebView2 tests either.

For the standalone [dashboard demo](DASHBOARD.md#browser-preview), run `npm run dev` and open the Vite URL. **Demo data** marks the in-memory fixture; it does not inspect Windows or exercise the real profile.

## Build-only packages

Use [Windows candidate packages (no publish)](WINDOWS-RELEASE.md#1-build-a-candidate-without-publishing) to retain the tested installer and ZIP artifacts. To publish, the separate [Release workflow](WINDOWS-RELEASE.md#6-tag-and-publish-the-tested-candidate) promotes the exact candidate bytes after CI and static verification. Do not rebuild or reuse a release tag for validation.

## Verify published packages or collect client evidence

Use the [published-asset check](WINDOWS-RELEASE.md#7-verify-the-published-downloads-without-launching-them) for existing releases. Test its verifier locally with `python -B -m unittest discover -s scripts/tests -v` (Python 3.11+). It checks file hashes/provenance and package structure without launch. The [optional launch observer](WINDOWS-RELEASE.md#8-observe-first-launch-and-exit-on-a-disposable-client) requires a disposable Windows 10/11 x64 client; Windows Server CI only checks its syntax and server refusal. See the [evidence matrix](../AUDIT.md) before reporting results.

## IPC and the UI thread

The shell gates heavy operations and offers independent read-only WAL queries. Scan/Watch run off the UI thread, with throttled progress over Tauri Channels and cooperative cancellation. Closing stops cancellable operations and waits at safe boundaries for other work; it does not kill vendor uninstallers. Workspace commands belong to `main`, not the `log` WebView. See [architecture](ARCHITECTURE.md) for boundaries and [remediation limits](AUDIT-REMEDIATION.md#limits-to-keep-visible) for caps.

## Optional live-source lab

Use the [isolated lab runbook](LIVE-COLLECTION.md) and [driver development guide](../driver/README.md). That work is not part of ordinary desktop development, release verification, or client launch smoke tests.
