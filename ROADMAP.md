# ExorSystem roadmap

ExorSystem records system changes and helps people review the evidence before cleanup. The [audit](AUDIT.md) separates automated coverage from checks that require a Windows desktop or driver lab.

## Shipped on main

- **Foundation** ([PR #5](https://github.com/SeanShaffer88/ExorSystem/pull/5)): file, service and task observations; policy preview; recoverable file quarantine; automated checks and Windows packaging. [v0.1.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.1.0) published NSIS, MSI and portable ZIP builds with SHA-256 hashes.
- **Inventory and leftover review** ([PR #19](https://github.com/SeanShaffer88/ExorSystem/pull/19)): classic Windows programs, current-user MSIX packages, and opt-in portable entries. Registry, service and task suggestions are read-only. Provenance can find an unambiguous ASCII case-insensitive match without merging older records.
- **Driverless Watch and History** ([PR #20](https://github.com/SeanShaffer88/ExorSystem/pull/20), [PR #21](https://github.com/SeanShaffer88/ExorSystem/pull/21)): opt-in registry subtree snapshots, persistent Watch controls across tabs, and a link to the completed watch's History. History includes removals, evidence sources, literal path search, session filtering and paging.
- **Unelevated desktop and release verification** ([PR #22](https://github.com/SeanShaffer88/ExorSystem/pull/22)): per-user storage, guarded uninstall and quarantine, a build-only package workflow, and a Windows 10/11 test fixture. The [Windows Server package run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44247905086) passed for NSIS, MSI, and portable ZIP. Full interactive client testing remains open.
- **Workspace dashboard** ([PR #24](https://github.com/SeanShaffer88/ExorSystem/pull/24)): inventory, guided uninstall and script preview, split Change inspector, cleanup outcomes and relationship graph. A file-only Watch → inspector smoke test on a real Windows PC observed additions, modifications and removals, with different fingerprints on the edited file. This is partial smoke evidence, not a Windows 10/11 client matrix.
- **Watch coverage** ([PR #25](https://github.com/SeanShaffer88/ExorSystem/pull/25)): opt-in scopes, explicit baseline and comparison coverage, and exclusion of incomplete captures rather than treating them as removals. File-only Watch skips registry, service, and task checks.
- **Dockable workspace** ([PR #26](https://github.com/SeanShaffer88/ExorSystem/pull/26)): move and resize panels, save layouts, use a compact dock switcher, and retain Watch/Scan progress while switching tools. The activity log is separate from the read-only Script preview.
- **Stable package promotion** ([PR #23](https://github.com/SeanShaffer88/ExorSystem/pull/23), [PR #27](https://github.com/SeanShaffer88/ExorSystem/pull/27)): publish a retained candidate using its original SHA-256-verified packages rather than rebuilding. Passing CI and package verification are required; interactive testing is optional.

## Unreleased development

- **Retain comparison context:** History and installation reports show what a Watch compared, including skipped scopes and comparisons with zero changes. Scope coverage is saved with the observations; old sessions without coverage remain unknown. This is a local profile upgrade, not a change to v0.2.0 packages or a native Windows test result.

## Watch an install

- Enter full paths to the folders you want to watch. No folders are selected by default. Select Services or Scheduled tasks only if you want their read-only metadata checked.
- To opt in to registry tracking, enter up to eight specific Software subtrees, one per line. Use `HKCU\Software\Example` for the calling user, `HKLM\Software\Example` for the 64-bit machine view, or `HKLM32\Software\Example` for the 32-bit machine view. Leave it blank to skip the registry. Whole hives and whole Software roots are not accepted.
- Wait for the baseline before making changes. You can switch tabs and return to compare or cancel. The results distinguish checks that were not requested, skipped, partly covered, or compared. Unreadable scopes are skipped with a reason, not treated as empty.
- After **Compare snapshots**, choose **View recorded changes** to open the **Change inspector** filtered to the watch session. Use **Timeline** to see all observation types. Key paths and last-write metadata show what changed between checks, not which process changed it. Registry value names and contents are not collected, and short-lived changes between snapshots may be missed.

## Before publishing a Windows build

Pass Linux/Windows CI and the **Windows candidate packages (no publish)** workflow. Use [Release](docs/WINDOWS-RELEASE.md#6-tag-and-publish-the-tested-candidate) to verify and promote the retained packages from a commit reachable from `main`, without rebuilding or reusing a tag. Interactive client sign-off is not required.

### v0.2.0 release evidence

[v0.2.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.2.0) was published on **2026-11-18**. The [promotion run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44284479160) tagged `c3aa8c0c0c2d8ac20d107f62b91c38b5eb67b142` and published the retained NSIS installer, MSI, and portable ZIP without rebuilding. The release attaches `SHA256SUMS.txt` and `PACKAGE-CHECKS.json`; [CI](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44272786809) and [static package verification](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44272786871) passed for that source commit. Back up legacy data as described in the [release notes](docs/releases/v0.2.0.md) before upgrading.

### Optional Windows client testing

The full interactive matrix has **not run** for the v0.2.0 packages. Use the [Windows runbook](docs/WINDOWS-RELEASE.md), not a live user profile. If you run client tests, record the exact commit, package SHA-256, OS build, results, and logs in the [evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md).

- [ ] Verify installation/extraction, normal launch, WebView2, elevation behavior, upgrade/removal and data location on both Windows 10 and 11 x64 for each installer format where applicable.
- [ ] Verify real Watch → History, inventory and uninstall/review, quarantine/restore/purge, and Windows-specific failure drills with disposable fixture data.
- [ ] Verify dock moves and splitters, persistence, Watch/Scan continuity, keyboard navigation, Narrator, high contrast, 100%/150% DPI, and Activity log on both clients.

## Driver work is separate

The normal desktop release does not require a driver. Keep the [live-source lab gates](docs/LIVE-COLLECTION.md#distribution-gates-open) separate from desktop publishing. Real USN and registry callback capture, WDK build/load, Microsoft altitude assignment, and signing remain open. Neither live source is enabled in the v0.2.0 desktop release.
