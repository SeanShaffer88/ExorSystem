# ExorSystem

**Compare what an installation changed before deciding what to clean up.** ExorSystem is an open-source Windows desktop app for installation snapshots, system-change evidence, guided uninstall review, and recoverable file quarantine. The desktop runs as a normal user and does not require a driver.

## Downloads

Get [ExorSystem v0.3.0 for Windows x64](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.3.0) as an **NSIS installer** (`-Setup.exe`), **MSI**, or **portable ZIP**. Compare the download with the release's `SHA256SUMS.txt` before running it. Extract the portable ZIP to a local folder and open `ExorSystem.exe` from Explorer. Installers may prompt for UAC; **open the desktop normally**, not as Administrator.

**Before upgrading:** Close ExorSystem and back up the entire `%LOCALAPPDATA%\ExorSystem` profile, including quarantine. v0.3.0 upgrades the database to schema v5 on first launch. Its automatic pre-migration SQLite copy does **not** include held files. Keep any legacy `data` folder from v0.1.0; it is not imported. See the [upgrade and rollback notes](docs/releases/v0.3.0.md#before-you-upgrade).

[CI verified the published packages](AUDIT.md#v030-release-evidence) without installing or launching them. The interactive Windows 10/11 client matrix is **Not run**.

## What you can do

- **Compare an install.** In **Registry monitor**, choose specific folders and optionally registry Software subtrees. Take a baseline, make a change or run the installer yourself, and compare. Services and scheduled tasks are independent, read-only opt-ins. See which scopes were compared or skipped. Snapshots can miss short-lived changes and do not identify the process responsible.
- **Trace recorded evidence.** Open a Watch result in **Change inspector**. Filter changes by installation, path, kind, or UTC dates, and export matching observations as JSON. Follow recorded relationships or save an installation report. A relationship's confidence score is not proof of ownership.
- **Review installed apps and leftovers.** Browse classic Windows programs, current-user MSIX apps, and explicitly added portable apps. Preview supported vendor uninstall commands before running them. Use Windows Settings for per-user or unsupported uninstallers. Registry, service, and task findings are review-only.
- **Quarantine exact files.** Review the file list before approving a reversible move. Restore held files without overwriting an existing destination, make a verified profile backup, or separately confirm permanent deletion. Quarantine does not replace a backup.
- **Arrange your workspace.** Dock tools in Main, Right, or Bottom, save a layout, and keep Watch open alongside the inspector. Activity log is a separate window on Windows. Themes and keyboard controls are available; native accessibility and mixed-DPI behavior still need client testing.

Start with the [dashboard guide](docs/DASHBOARD.md#watch-an-install). To try Watch without an installer, use [a disposable file-only fixture](docs/WINDOWS-RELEASE.md#3-verify-watch-and-history-with-real-files-and-registry-keys).

## Documentation

| If you want to… | Start here |
|---|---|
| Use Watch, History, cleanup, or backup | [Dashboard guide](docs/DASHBOARD.md) |
| Build or test a release, or collect native evidence | [Windows runbook](docs/WINDOWS-RELEASE.md) · [Evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md) |
| See what shipped or what is still open | [Roadmap](ROADMAP.md) · [Changelog](CHANGELOG.md) · [Verification matrix](AUDIT.md) |
| Work on the code | [Development](docs/DEVELOPMENT.md) · [Architecture](docs/ARCHITECTURE.md) |

See the [full documentation index](docs/README.md) for release notes and the separate experimental live-source lab.

## Build and test

From the repository root, with the [pinned toolchain](docs/DEVELOPMENT.md):

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
npm --prefix gui ci
npm --prefix gui run build
```

The read-only command line builds with `cargo build -p exor-cli`; see [docs/CLI.md](docs/CLI.md).

The frontend builds separately from the desktop. To run the browser demo, use `npm --prefix gui run dev` and open the Vite URL. Its **Demo data** badge marks fixture data, not Windows observations. For the Windows desktop, see [development setup](docs/DEVELOPMENT.md#run-the-desktop) or the [package build instructions](docs/WINDOWS-RELEASE.md#1-build-a-candidate-without-publishing).

## Data

All three package formats use the current Windows user's `%LOCALAPPDATA%\ExorSystem` for the SQLite profile and quarantine. The desktop has no telemetry. It stores recorded paths, file fingerprints, and comparison coverage; Watch does not record registry values. Reports and exports can include local paths, so review them before sharing. The Activity log is session-only and is not the evidence ledger. Windows prerequisites and vendor uninstallers are separate from the app's local evidence store.

## License

[MIT](LICENSE). See [credits and dependency licensing](CREDITS.md).
