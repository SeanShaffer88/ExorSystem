# Roadmap and release checks

The original implementation roadmap shipped in [PR #5](https://github.com/SeanShaffer88/ExorSystem/pull/5): service/task collectors, recoverable quarantine, the quarantine UI, CI, and Windows packaging. The [verification matrix](../AUDIT.md) distinguishes working code from behavior verified on Windows.

## Inventory and review follow-up

- **Installations** includes classic registry entries and main MSIX/AppX packages for the calling Windows user. Use **Show system components** to include system-signed packages. A package-source error is shown without hiding the rest of the inventory. Manage MSIX apps in Windows Settings; ExorSystem does not uninstall them.
- **Settings → Portable apps** adds or forgets a user-chosen local `.exe` in the inventory. It does not execute, copy, or remove the file, or search the disk for portable apps. Entries are saved in the local database.
- **Installations → Find leftovers** works for classic programs even without a recorded install. If ExorSystem runs the uninstaller and Windows removes the entry, it retains non-command metadata for review until the app closes. Folder matches are suggestions; only manually selected folders inside application roots are eligible for file quarantine. Registry keys, services and scheduled tasks are review-only. An exact name or executable-location match is not proof of ownership.
- **Provenance** prefers an exact path. It also finds an unambiguous ASCII case-insensitive match and asks for the exact spelling if older records differ only in case. Existing nodes and ledger entries are not rewritten.

## Before publishing a Windows build

Use disposable Windows 10 and 11 x64 VMs. Do not run uninstall or failure drills against a real user's files. Record the commit, OS build, pass/fail, and any logs in the release notes or audit. None of these desktop checks is replaced by the browser smoke test.

- [ ] Pass Linux/Windows CI, including the built-frontend smoke test and native Tauri check. Resolve the separate [elevated cleanup hardening review](https://github.com/SeanShaffer88/ExorSystem/pull/18) before shipping changes to privileged cleanup.
- [ ] Build locally with `./scripts/release.ps1`; verify the SHA-256 list, then try the portable ZIP, NSIS installer, and MSI on clean VMs. Check the UAC prompt, upgrade backup, and WebView2 on Windows 10.
- [ ] Compare Installations with Windows Settings for a known classic program and a current-user MSIX app. Verify the MSIX app is read-only. Add a disposable portable `.exe`, restart and check that it persists, then forget it and confirm its files are unchanged.
- [ ] Use an expendable classic app that removes its Windows uninstall entry. Check that it stays visible for review, but cannot be launched again. Verify a matching registry key, service, and task appear as advice only and that unrelated entries do not appear. Quarantine a selected test folder and restore it. Check a locked file and a junction using only disposable data.
- [ ] Record a test file with **Watch an install** and trace a differently cased path to it. Upgrade a copy of a v2 database and confirm the v2 backup and ledger are intact.
- [ ] Verify keyboard navigation, Narrator, high contrast, 100%/150% DPI, and activity-log docking on two monitors. See [screenshot capture list](SCREENSHOTS.md) for actual Windows imagery.

## Driver work is separate

The standard application and these checks do not require the optional registry driver. WDK build and load verification, Microsoft altitude assignment, and signing are separate gates before distributing a driver. Use the [driver development instructions](../driver/README.md) only on a disposable VM.
