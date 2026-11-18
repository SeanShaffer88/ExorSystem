# ExorSystem roadmap

ExorSystem records system changes and helps people review the evidence before cleanup. The [audit](AUDIT.md) separates automated coverage from checks that require a Windows desktop or driver lab.

## Shipped on main

- **Foundation** ([PR #5](https://github.com/SeanShaffer88/ExorSystem/pull/5)): file, service and task observations; policy preview; recoverable file quarantine; automated checks and Windows packaging. [v0.1.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.1.0) published NSIS, MSI and portable ZIP builds with SHA-256 hashes.
- **Inventory and leftover review** ([PR #19](https://github.com/SeanShaffer88/ExorSystem/pull/19)): classic Windows programs, current-user MSIX packages, and opt-in portable entries. Registry, service and task suggestions are read-only. Provenance can find an unambiguous ASCII case-insensitive match without merging older records.
- **Driverless Watch and History** ([PR #20](https://github.com/SeanShaffer88/ExorSystem/pull/20), [PR #21](https://github.com/SeanShaffer88/ExorSystem/pull/21)): opt-in registry subtree snapshots, persistent Watch controls across tabs, and a link to the completed watch's History. History includes removals, evidence sources, literal path search, session filtering and paging.
- **Unelevated desktop and release verification** ([PR #22](https://github.com/SeanShaffer88/ExorSystem/pull/22)): per-user storage, guarded uninstall and quarantine, a build-only package workflow, and a Windows 10/11 test fixture. The [Windows Server package run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44247905086) passed for NSIS, MSI, and portable ZIP. Interactive client sign-off remains open.
- **Workspace dashboard** ([PR #24](https://github.com/SeanShaffer88/ExorSystem/pull/24)): inventory, guided uninstall and script preview, split Change inspector, cleanup outcomes and relationship graph. A file-only Watch → inspector smoke test on a real Windows PC observed additions, modifications and removals, with different fingerprints on the edited file. This is partial evidence, not exact-package Windows 10/11 sign-off.

## Development priorities

| Roadmap item | Delivered | Remaining verification |
|---|---|---|
| Review install evidence | Session-scoped History, storage regression tests and built-bundle browser smoke | Run Watch → History against real HKCU and file changes on Windows 10 and 11, including removals and two separate sessions. |
| Cleanup boundaries ([original review #18](https://github.com/SeanShaffer88/ExorSystem/pull/18)) | Remove desktop elevation instead of adding privileged storage. Enforce live machine-registration uninstall commands and use a new per-user profile; preserve older portable data. Derive quarantine paths by ID, refuse links and overwrites, and recover both cleanup entry points after failed record/ledger writes. | Exercise standard/elevated startup, the supported classic uninstall path and cleanup failure drills in the native release matrix. |
| Repeatable release checks | Build-only package workflow, static artifact inspection, disposable Windows fixture, end-to-end [runbook](docs/WINDOWS-RELEASE.md) and [evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md) | Complete and attach Windows 10/11 results for NSIS, MSI and portable ZIP. Do not replace desktop checks with Windows Server CI. |
| Optional live collection | Separate [source review and lab probe](docs/LIVE-COLLECTION.md); neither source is enabled in the desktop | Validate real USN and registry callback capture separately. Do not ship the driver without the WDK, altitude and signing gates. |

## Watch an install

- Enter full paths to the folders you want to watch. No folders are selected by default. Select Services or Scheduled tasks only if you want their read-only metadata checked.
- To opt in to registry tracking, enter up to eight specific Software subtrees, one per line. Use `HKCU\Software\Example` for the calling user, `HKLM\Software\Example` for the 64-bit machine view, or `HKLM32\Software\Example` for the 32-bit machine view. Leave it blank to skip the registry. Whole hives and whole Software roots are not accepted.
- Wait for the baseline before making changes. You can switch tabs and return to compare or cancel. The results distinguish checks that were not requested, skipped, partly covered, or compared. Unreadable scopes are skipped with a reason, not treated as empty.
- After **Compare snapshots**, choose **View recorded changes** to open the **Change inspector** filtered to the watch session. Use **Timeline** to see all observation types. Key paths and last-write metadata show what changed between checks, not which process changed it. Registry value names and contents are not collected, and short-lived changes between snapshots may be missed.

## Before publishing a Windows build

Use the [Windows runbook](docs/WINDOWS-RELEASE.md), not a live user profile. Check the exact candidate commit and record each package's SHA-256, OS build, results, and logs in the [evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md).

- Pass Linux/Windows CI and the **Windows candidate packages (no publish)** workflow. These are automated gates, not desktop sign-off.
- [ ] Verify installation/extraction, normal launch, WebView2, elevation behavior, upgrade/removal and data location on both Windows 10 and 11 x64 for each installer format where applicable.
- [ ] Verify real Watch → History, inventory and uninstall/review, quarantine/restore/purge, and Windows-specific failure drills with disposable fixture data.
- [ ] Verify keyboard navigation, Narrator, high contrast, 100%/150% DPI, and activity-log docking across two monitors. Attach completed evidence before [tagging and publishing the same packages](docs/WINDOWS-RELEASE.md#6-tag-and-publish-the-tested-candidate).

## Driver work is separate

The normal desktop release does not require a driver. Keep the [live-source lab gates](docs/LIVE-COLLECTION.md#distribution-gates-open) separate from desktop sign-off. WDK build/load, Microsoft altitude assignment, and signing remain open.
