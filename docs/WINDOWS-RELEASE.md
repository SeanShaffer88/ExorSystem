# Windows release verification

**Stable publishing requires passing CI and verified candidate packages. Interactive Windows 10/11 testing is optional, not a release gate.**

For the optional client checks in sections 2–5, use **disposable Windows 10 and 11 x64 VMs** and a normal test account. Use a clean snapshot for each installer format. Keep unrelated user files off the VMs. An Actions Windows Server runner proves compilation and package structure, not Windows client behavior. The steps below use [dashboard](DASHBOARD.md) labels. Earlier builds call Applications **Installations**, show **Watch an install** directly, and expose **History**, **Provenance**, and **Quarantine** as standalone tabs.

## 1. Build a candidate without publishing

Use the **Windows candidate packages (no publish)** Actions workflow on the candidate commit, or build on a Windows machine with the [development toolchain](DEVELOPMENT.md):

```powershell
./scripts/release.ps1 -OutDir ./dist
./scripts/check-release.ps1 -ArtifactDir ./dist
```

Use the **workflow artifact** for publication and any optional client testing. Keep `SHA256SUMS.txt` and `PACKAGE-CHECKS.json` with the three packages. The verification script checks their hashes, the NSIS PE header, the MSI x64 template, and the portable ZIP contents plus its executable's x64, `asInvoker`, and PerMonitorV2 manifest. It does **not** install or launch the packages. Local builds are useful for troubleshooting but are not a substitute for the exact workflow artifact you intend to release.

## 2. Install, start, and remove each format

Use artifacts from one exact commit, and verify their SHA-256 before running them. Keep results for every OS/format pair.

| Format | What to exercise |
|---|---|
| Portable ZIP | Extract to a local path with spaces. Launch and relaunch `ExorSystem.exe` from Explorer. No installer or adjacent `data` directory should be created. Removing the extracted application must not remove its user profile. |
| NSIS EXE | Verify the visible install flow, Start-menu shortcut, actual binary launch, an upgrade from the previous version, and uninstall via Windows Settings. Capture installer dialogs and any exit errors. |
| MSI | Install via Explorer or `msiexec.exe /i "<candidate.msi>" /L*v "<install.log>"`. Verify the shortcut and installed launch, upgrade from the previous version (with a different package version), and uninstall. Keep verbose MSI logs. Also verify the installer's launch-after-install option if offered. |

For each format:

1. Verify the desktop does **not** prompt for UAC and runs unelevated. Installing or removing a machine-wide package may prompt separately. If an installer run from an elevated terminal launches an elevated copy, confirm ExorSystem refuses to open and can be launched normally afterward. With a fresh profile, deliberately launch as Administrator once: expect a clear refusal and no new profile or data writes. A second normal instance should report that the profile is in use.
2. Verify WebView2 loads the actual desktop, including on a Windows 10 VM without a preinstalled runtime. Note the runtime version and what installed it, or record the missing-runtime error. Record SmartScreen/signature behavior without turning off security controls.
3. In **Settings**, confirm the database and quarantine are under the normal account's `%LOCALAPPDATA%\ExorSystem`. Close and relaunch to check persistence. For the legacy-data test, make a disposable `data` directory beside a portable copy with a marker file. Confirm Settings mentions it, it is not read as the new profile, and its hashes are unchanged after restart. Legacy data is deliberately not auto-imported. The existing v2-to-v3 database upgrade, backup, and ledger preservation have separate `exor-storage` tests.

## 3. Verify Watch and History with real files and registry keys

From a non-elevated PowerShell in the checkout on the VM:

```powershell
$fixture = ./scripts/windows-fixture.ps1 -Action New
$fixture | Format-List
```

The script prints an ID, file folders, exact registry subtrees, a per-user inventory name, and a portable executable path. It changes only its own files and HKCU keys; it does not install anything or alter a service, scheduled task, or driver.

1. In **Registry monitor → Watch an install**, set a distinct name ending in `A`, enter `$fixture.folder_a` in Folders and `$fixture.registry_a` in Registry subtrees. Leave Services and Scheduled tasks unchecked for this test. Select **Take baseline**. Verify File system and Registry baselines are ready and both system checks say **Not requested**, then run `./scripts/windows-fixture.ps1 -Action ChangeA -Id $fixture.id`. Switch to another tab and back, then select **Compare snapshots**. Verify File system and Registry were compared and choose **View recorded changes**. Older builds have default folders to replace, check services/tasks automatically, and call these buttons **Take the first picture** and **Finish: take the second picture**.
2. The Change inspector’s Installation filter must show that Watch's session ID. Expect `a-created.txt`, `a-change.txt`, and removal of `a-gone.txt`; the registry `Added`, `Changed`, and removed `Gone` keys; and **Snapshot diff** as the source. A parent key may also change. No registry value data should appear. Focus should land on Change inspector. Confirm the split panes show registry and files separately, the file modification exposes the recorded before/after fingerprints, and Timeline still shows unassigned entries when the session filter is cleared. Trace one recorded file with a different ASCII case and verify it resolves to the recorded item.
3. Repeat with a different name ending in `B`, `$fixture.folder_b`, and `$fixture.registry_b` (initially missing). Between pictures run `./scripts/windows-fixture.ps1 -Action ChangeB -Id $fixture.id`. History for B must include the newly created Second key and B's files, not A's. Use **Older changes** and **Newer changes**; then search for `percent%_literal`, which should match exactly the fixture file with those literal characters. Switch to A, then to **All installations and unassigned changes** and verify the expected separation and combination of records. `-Action Verify -Id $fixture.id` confirms the input fixtures, not the desktop output.
4. On builds with saved Watch coverage, restart ExorSystem and reselect session A in **Change inspector**. Confirm the scope counts and skip details match the original comparison. Change the path filter to one with no matching rows and verify it does not change the coverage. A pre-upgrade session has no saved comparison. In **Applications**, save the matching session’s report and check its coverage totals.
5. Optional over-limit input: `./scripts/windows-fixture.ps1 -Action MakeOversized -Id $fixture.id`. Take before/after pictures of `$fixture.oversized`; expect an incomplete-scope warning, not thousands of changes. Test **Cancel watch** with a new Watch. In a **Thorough** cleanup preview, registry observations must never be selected for file quarantine.

If you lack a VM, a **file-only smoke test** on a normal Windows account can exercise Watch without touching installed apps. Create a new folder under `%TEMP%` with two disposable files. Enter only that folder, leave the registry and system checks off, take the baseline, then edit one file, add one and remove the other original. Compare and verify one Added, Changed and Removed entry and different before/after fingerprints for the edited file. Record the OS build, original package hash, and result as **partial smoke evidence**. Do not run the uninstall and cleanup drills on a live profile or report a file-only smoke as coverage of the full client matrix.

## 4. Verify inventory, cleanup and recovery

- Compare a classic entry and a current-user MSIX app with Windows Settings. MSIX and the fixture's HKCU program must have no ExorSystem uninstall action. The HKCU program must still support **Review leftovers → Find leftovers**. Add `$fixture.portable_exe` as a portable app, restart, forget the entry, and verify the executable's SHA-256 is unchanged. Do not run the fixture's uninstall entry from Windows Settings.
- Use an expendable machine-wide classic app for the positive uninstall case. ExorSystem must show the registered executable and arguments, then require confirmation before launching, not execute on selection. Check cancellation/nonzero exit and a successful uninstall if the program supports being launched without elevation. After its registry entry disappears, it must be review-only for the rest of the session. For an executable that requires elevation or a command format ExorSystem refuses, expect guidance to use Windows Settings, not a privileged retry. Record the chosen program/version and result; do not substitute a live user app.
- Select only the test folder in **Review leftovers → Find leftovers** for `$fixture.display_name`. Registry, service, and task suggestions are advisory, not selectable for removal. Record the SHA-256 of fixture A's files. Quarantine the selection and confirm they disappear. Recreate `a-change.txt` with different bytes, then attempt Restore. The new file must stay unchanged and the held batch must remain available. Remove only the fixture conflict and retry; restored SHA-256 values must match. Use another disposable batch to verify **Delete permanently** requires its final confirmation and retains a manifest.
- In a second terminal, run `./scripts/windows-fixture.ps1 -Action LockFile -Id $fixture.id`, then try quarantining the A folder. Expect failure without losing either the locked file or any earlier file. Release the lock and verify originals before retrying. For a junction drill, make a junction inside A targeting B (both fixture paths); confirm B is unaffected by quarantining A. Remove only the junction, or revert the snapshot. Do not point it at system paths.
- Run `cargo test -p exor-engine --locked` on disposable Windows if you need to repeat the database write, sharing violation, reparse, and interrupted-batch drills. The tests inject record/ledger failures in their own databases. They do not tamper with the desktop's profile. A real machine shutdown during quarantine, cross-volume NTFS restore, and long-path behavior remain separate native checks; record any skipped scenario explicitly.

Close ExorSystem, finish or restore its fixture batches and forget the portable entry before removing the fixture. Use `./scripts/windows-fixture.ps1 -Action Remove -Id $fixture.id` or revert the VM snapshot. The script does not recursively remove fixture file folders or touch the desktop database.

## 5. Accessibility and optional client evidence

### Docking and accessibility, on each Windows client

Use the exact verified package and a normal test profile. See the [workspace controls](DASHBOARD.md#arrange-the-workspace).

1. Apply **Investigation layout**. Verify Main, Right dock, and Bottom dock each show the selected panel and scroll independently. Drag a tab into another dock, including one that is empty, and reorder tabs. Dropping a disposable Explorer file onto the workspace should not open it or change the layout. Move the same panel back using only Tab/Shift+Tab and **Dock [panel name]** → destination. Resize both splits by mouse and keyboard; confirm focus lands on the moved tab and remains usable after **Close** or **Delete**. Reopen a closed tool via the sidebar and check it returns to its previous dock.
2. With the file-only Watch fixture, take a baseline. Move or close and reopen **Registry monitor** before finishing. Confirm there is still one baseline and no extra Watch, then compare. Move the panel while the comparison is running if it takes long enough. From **View recorded changes**, check that an already-open Change inspector switches to the correct session and receives focus. Move a Scan folder panel with a draft or an active scan and verify progress/result where observable. Review a supported **Script preview** from Applications when the preview is already docked; no command launches. Open the wizard without running anything and check that workspace shortcuts do not bypass its modal review.
3. Resize or snap the window until the dock switcher appears. Verify all three docks are reachable and no controls are clipped; expand to restore split widths. Close and launch from the same normal profile and verify placement, order, active tabs and splitter sizes persist. Apply **Cleanup layout** and **Reset layout**; verify history and batches are unchanged. An applied theme should persist, an unapplied preview should end when hidden, and **Undo** should restore the previous theme.
4. On builds with the bottom attachment, open **Activity log** from the sidebar, **Log**, and **Ctrl+`**. Confirm it is a separate window with no duplicate dock tab. On a normal window, move and resize, including near the monitor’s bottom edge; it should stay below the main window and clear the taskbar. Minimize/restore both. Maximize with the log open and also open it while already maximized. In each case close the log (button and **Alt+F4**) and verify the main window fills the work area, remains maximized, and can still restore to its normal size. Move across two monitors at different scaling settings, if available. In Applications, select a row near the bottom of a page with long registration or evidence; scroll to the last action and use **Back to applications** in a narrow dock. Clear the log and verify the status bar resets without affecting Change inspector; then produce another disposable scan event. For the older v0.2.0 layout, test the in-app log and optional right-side attachment instead. Record any scenarios not run.
5. Use Narrator and Windows high contrast; verify tool, dock, tab, separator, and modal names, keyboard focus, panel transitions, and Watch status. Check 100% and 150% display scaling, including the minimum window size. Record actual OS and WebView2 versions, scale and monitor setup. Use fixture paths and redact personal data in screenshots (see the [capture list](SCREENSHOTS.md)).

If you run client checks, use the [evidence template](WINDOWS-RELEASE-EVIDENCE.md) to record the commit, per-format hash, OS build, VM setup, WebView2 version, status, and logs. Attach the record to a PR or issue. Mark anything not run **Not run**, not Pass. The [live-source/driver gates](LIVE-COLLECTION.md) are separate and do not block driverless desktop packaging.

## 6. Tag and publish the tested candidate

Use the separate **Release** workflow to publish. It does not run automatically, and does not require a client evidence link or a sign-off checkbox.

1. Pick a successful **Windows candidate packages (no publish)** run from this repository with a retained artifact. Its exact source commit must be reachable from `main` and have passing CI. If a squash merge made the candidate commit unreachable, run CI and build a candidate on the landed commit instead. If the artifact expired, rerun the candidate workflow and use the new run ID.
2. In Actions → **Release** → **Run workflow**, select the default branch and enter `candidate_run_id` (the number in the run URL). Do not create the tag first or use this workflow just to build packages.
3. The workflow downloads the artifact and checks its version, run ID, source commit, sizes, and SHA-256 hashes. It refuses to reuse an existing tag, creates `v{version}` at the candidate commit, and publishes the **same bytes**, checksums, and package report without rebuilding. Check the new release and tag target afterward. The release notes link the automated checks and do not imply a Windows client pass.
