# Windows release verification

Run these checks on **disposable Windows 10 and 11 x64 VMs**, from a normal test account. Use a clean snapshot for each installer format. Keep unrelated user files off the VMs. An Actions Windows Server runner proves compilation and package structure, not Windows client behavior. The steps below use [dashboard](DASHBOARD.md) labels. Earlier builds call Applications **Installations**, show **Watch an install** directly, and expose **History**, **Provenance**, and **Quarantine** as standalone tabs.

## 1. Build a candidate without publishing

Use the **Windows candidate packages (no publish)** Actions workflow on the candidate commit, or build on a Windows machine with the [development toolchain](DEVELOPMENT.md):

```powershell
./scripts/release.ps1 -OutDir ./dist
./scripts/check-release.ps1 -ArtifactDir ./dist
```

Use the **workflow artifact** for sign-off and eventual publication. Keep `SHA256SUMS.txt` and `PACKAGE-CHECKS.json` with the three packages. The verification script checks their hashes, the NSIS PE header, the MSI x64 template, and the portable ZIP contents plus its executable's x64, `asInvoker`, and PerMonitorV2 manifest. It does **not** install or launch the packages. Local builds are useful for troubleshooting but are not a substitute for the exact workflow artifact you intend to release.

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
4. Optional over-limit input: `./scripts/windows-fixture.ps1 -Action MakeOversized -Id $fixture.id`. Take before/after pictures of `$fixture.oversized`; expect an incomplete-scope warning, not thousands of changes. Test **Cancel watch** with a new Watch. In a **Thorough** cleanup preview, registry observations must never be selected for file quarantine.

If you lack a VM, a **file-only smoke test** on a normal Windows account can exercise Watch without touching installed apps. Create a new folder under `%TEMP%` with two disposable files. Enter only that folder, leave the registry and system checks off, take the baseline, then edit one file, add one and remove the other original. Compare and verify one Added, Changed and Removed entry and different before/after fingerprints for the edited file. Record the OS build, original package hash, and result as **partial smoke evidence**. Do not run the uninstall and cleanup drills on a live profile or treat this test as client sign-off.

## 4. Verify inventory, cleanup and recovery

- Compare a classic entry and a current-user MSIX app with Windows Settings. MSIX and the fixture's HKCU program must have no ExorSystem uninstall action. The HKCU program must still support **Review leftovers → Find leftovers**. Add `$fixture.portable_exe` as a portable app, restart, forget the entry, and verify the executable's SHA-256 is unchanged. Do not run the fixture's uninstall entry from Windows Settings.
- Use an expendable machine-wide classic app for the positive uninstall case. ExorSystem must show the registered executable and arguments, then require confirmation before launching, not execute on selection. Check cancellation/nonzero exit and a successful uninstall if the program supports being launched without elevation. After its registry entry disappears, it must be review-only for the rest of the session. For an executable that requires elevation or a command format ExorSystem refuses, expect guidance to use Windows Settings, not a privileged retry. Record the chosen program/version and result; do not substitute a live user app.
- Select only the test folder in **Review leftovers → Find leftovers** for `$fixture.display_name`. Registry, service, and task suggestions are advisory, not selectable for removal. Record the SHA-256 of fixture A's files. Quarantine the selection and confirm they disappear. Recreate `a-change.txt` with different bytes, then attempt Restore. The new file must stay unchanged and the held batch must remain available. Remove only the fixture conflict and retry; restored SHA-256 values must match. Use another disposable batch to verify **Delete permanently** requires its final confirmation and retains a manifest.
- In a second terminal, run `./scripts/windows-fixture.ps1 -Action LockFile -Id $fixture.id`, then try quarantining the A folder. Expect failure without losing either the locked file or any earlier file. Release the lock and verify originals before retrying. For a junction drill, make a junction inside A targeting B (both fixture paths); confirm B is unaffected by quarantining A. Remove only the junction, or revert the snapshot. Do not point it at system paths.
- Run `cargo test -p exor-engine --locked` on disposable Windows if you need to repeat the database write, sharing violation, reparse, and interrupted-batch drills. The tests inject record/ledger failures in their own databases. They do not tamper with the desktop's profile. A real machine shutdown during quarantine, cross-volume NTFS restore, and long-path behavior remain separate native checks; record any skipped scenario explicitly.

Close ExorSystem, finish or restore its fixture batches and forget the portable entry before removing the fixture. Use `./scripts/windows-fixture.ps1 -Action Remove -Id $fixture.id` or revert the VM snapshot. The script does not recursively remove fixture file folders or touch the desktop database.

## 5. Accessibility and final evidence

Check keyboard navigation (Tab, Shift+Tab, arrow keys and Home/End on tabs), visible focus, Change inspector focus after Watch, Narrator names and status announcements, high contrast, 100%/150% DPI and moving the app with the activity log across two monitors. Include screenshots or short notes; see the [capture list](SCREENSHOTS.md).

Copy the [evidence template](WINDOWS-RELEASE-EVIDENCE.md) for this candidate. Record the commit and per-format hash, OS edition/build, VM setup, WebView2 version, status, and logs. Attach the completed record to the PR or release checklist. Mark anything not run **Not run**, not Pass. Publish only when the required native checks pass; the [live-source/driver gates](LIVE-COLLECTION.md) are separate and do not block driverless desktop packaging.

## 6. Tag and publish the tested candidate

Do not create a tag before both Windows client matrices pass. Use the separate **Release** workflow from the default branch only after recording sign-off:

1. Attach the completed evidence record to a PR or issue in this repository. Include the successful Windows candidate **Actions run ID**, source commit, and the SHA-256 of every tested package. The tested commit must be reachable from `main` and have passing CI. If it was squash-merged and is no longer reachable, build and sign off the candidate on the new commit.
2. In Actions → **Release** → **Run workflow**, enter `candidate_run_id` (the number in the run URL) and `client_evidence_url` (the PR, issue, or comment containing the completed results). Check `client_signoff` only after verifying the results against the exact artifact. The checkbox is the operator's sign-off, not an automated client test.
3. The workflow downloads the retained artifact from that run, verifies its version, source, sizes, and hashes, then creates the new `v{version}` tag and publishes the **same bytes** along with the checksums and evidence. It refuses to reuse a tag and does not rebuild. Check the new release and its tag target afterward. If the artifact has expired, build a new candidate and verify the new package hashes on both clients before publishing.

Do not use **Release** just to build packages. Until the client results are complete, leave the candidate in Actions and keep the latest stable release unchanged.
