# ExorSystem roadmap

ExorSystem records system changes and helps people review the evidence before cleanup. This roadmap separates work that has landed on `main` from verification still needed before the next Windows release. See the [verification matrix](AUDIT.md) and [development instructions](docs/DEVELOPMENT.md) for the evidence behind those statuses.

## Shipped on main

- **Foundation** ([PR #5](https://github.com/SeanShaffer88/ExorSystem/pull/5)): file, service and task observations; policy preview; recoverable file quarantine; automated checks and Windows packaging. [v0.1.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.1.0) published NSIS, MSI and portable ZIP builds with SHA-256 hashes.
- **Inventory and leftover review** ([PR #19](https://github.com/SeanShaffer88/ExorSystem/pull/19)): classic Windows programs, current-user MSIX packages, and opt-in portable entries. Guided classic uninstalls can review leftovers after the uninstall entry disappears. Registry, service and task suggestions are read-only. Provenance can find an unambiguous ASCII case-insensitive match without merging older records.
- **Driverless Watch** ([PR #20](https://github.com/SeanShaffer88/ExorSystem/pull/20)): opt-in registry subtree snapshots join file, service and task observations. The Watch tab keeps its controls and result when you switch tabs. Automated tests passed; interactive Windows testing is still pending.

## Development priorities

1. **Review the evidence from an install.** Watch results link to the correct session in History, where users can filter by path, page back through changes, and see the source of each observation. Verify the flow on Windows, including a removed registry key, and confirm that records from other sessions do not appear in the filtered view.
2. **Harden privileged cleanup before releasing changes to it.** Resolve the separate [elevated cleanup review](https://github.com/SeanShaffer88/ExorSystem/pull/18). Verify uninstall command boundaries, quarantine storage and restore behavior, and recovery after a failed write using disposable data. Observations never authorize registry, service or task removal.
3. **Make native release checks repeatable.** Run the checklist below on Windows 10 and 11. Document disposable test fixtures and attach results for each OS and installer format so the next release can repeat the same checks.
4. **Evaluate optional live collection separately.** Validate the USN and registry-driver sources before any desktop integration. Keep the default desktop workflow driverless. Driver distribution has its own gates below.

## Watch an install

- To opt in to registry tracking, enter up to eight specific Software subtrees, one per line. Use `HKCU\Software\Example` for the calling user, `HKLM\Software\Example` for the 64-bit machine view, or `HKLM32\Software\Example` for the 32-bit machine view. Leave it blank to skip the registry. Whole hives and whole Software roots are not accepted.
- Wait for the baseline before running the installer. You can switch tabs and return to finish or cancel. Subtrees that could not be fully read at either end are skipped with a warning, not treated as empty.
- After Finish, choose **View recorded changes** to open **History** filtered to the watch session. Entries include removals and their evidence source. Key paths and last-write metadata show what changed between checks, not which process changed it. Registry value names and contents are not collected, and short-lived changes between snapshots may be missed.

## Before publishing a Windows build

Use disposable Windows 10 and 11 x64 VMs. Do not run uninstall or failure drills against a real user's files. Record the commit, OS build, installer format, pass/fail, and relevant logs. Browser smoke tests and CI do not replace interactive desktop checks.

- [ ] Pass Linux/Windows CI, including the built frontend smoke test and Tauri shell check. Complete the privileged cleanup review before shipping changes to those operations.
- [ ] Build locally with `./scripts/release.ps1`; verify the SHA-256 list, then try the portable ZIP, NSIS installer and MSI on clean VMs. Check the UAC prompt, data location, and WebView2 on Windows 10.
- [ ] Compare Installations with Windows Settings for a known classic program and a current-user MSIX app. Verify the MSIX entry is read-only. Add a disposable portable `.exe`, restart and check for persistence, then forget it and confirm its files are unchanged.
- [ ] Uninstall an expendable classic app that removes its Windows uninstall entry. Verify it remains available for leftover review but cannot be launched again. Check that registry, service and task matches are advice only. Quarantine and restore explicitly selected test files; check handling of locked files and junctions with disposable data.
- [ ] Watch a test file and trace it using a differently cased path. Upgrade a copy of a v2 database and confirm its backup and previous ledger are intact.
- [ ] Without a driver, watch a disposable `HKCU\Software\ExorSystemWatchFixture` subtree. Between pictures, add and remove keys and edit a value in an existing key. Switch tabs and return to Finish, then open History from the result. Check for key-level evidence including the removal, snapshot source labels, and the expected session filter. Verify that an unreadable or oversized subtree warns, a missing starting key can be recorded as added, and the Thorough cleanup preview never proposes registry quarantine.
- [ ] Verify keyboard navigation, Narrator, high contrast, 100%/150% DPI, and activity-log docking on two monitors. See the [screenshot capture list](docs/SCREENSHOTS.md).

## Driver work is separate

The standard application and release checks above do not require the optional registry driver. Before distributing a driver, verify WDK build and load, Microsoft altitude assignment, and signing. Follow the [driver development instructions](driver/README.md) only on a disposable VM.
