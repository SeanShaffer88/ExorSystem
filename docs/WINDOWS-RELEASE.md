# Windows release verification

**Future stable promotion requires passing CI, exact retained-candidate verification, and reviewed Windows 10/11 x64 client sign-off on those bytes.** This gate does not change existing releases.

For [v0.3.0](releases/v0.3.0.md), the [automated release and published-asset checks](../AUDIT.md#v030-release-evidence) passed. The native Windows 10/11 matrix is **Not run**. Choose the checks that match your goal:

| Goal | Start here |
|---|---|
| Build or promote a new candidate | [Build without publishing](#1-build-a-candidate-without-publishing) · [Promote exact bytes](#6-tag-and-publish-the-tested-candidate) |
| Verify the current release without launch | [Check the published downloads](#7-verify-the-published-downloads-without-launching-them) |
| Record first launch and exit only | [Observe a verified executable](#8-observe-first-launch-and-exit-on-a-disposable-client) |
| Exercise the installed desktop | [Install/extract and upgrade](#2-install-start-and-remove-each-format), then Watch, cleanup, and accessibility in sections 3–5 |

For all client checks, use **disposable Windows 10 and 11 x64 VMs** and a normal test account. Start from a clean snapshot for each NSIS, MSI, or portable test. Keep unrelated user data off the VMs. Windows Server can validate code and packages, not the client matrix. The steps use the current [dashboard](DASHBOARD.md) labels.

## 1. Build a candidate without publishing

Use the **Windows candidate packages (no publish)** Actions workflow on the candidate commit, or build on a Windows machine with the [development toolchain](DEVELOPMENT.md):

```powershell
./scripts/release.ps1 -OutDir ./dist
./scripts/check-release.ps1 -ArtifactDir ./dist
```

Use the **workflow artifact** for publication and for testing an unpublished candidate. Before building a new stable version, align the workspace, desktop, Tauri and frontend versions and add `docs/releases/vX.Y.Z.md` to the candidate source. When testing an already released version, [verify the exact published downloads](#7-verify-the-published-downloads-without-launching-them) instead. Keep `SHA256SUMS.txt` and `PACKAGE-CHECKS.json` with the three packages. The verification script checks their hashes, the NSIS PE header, the MSI x64 template, and the portable ZIP contents plus its executable's x64, `asInvoker`, and PerMonitorV2 manifest. It does **not** install or launch the packages. Local builds are useful for troubleshooting but are not a substitute for the exact workflow artifact you intend to release.

## 2. Install, start, and remove each format

Use artifacts from one exact commit for the target version, and verify their SHA-256 before running them. For a released build, start with section 7. Keep results for every OS/format pair. If recording the optional first-launch smoke for a published release, decline any launch-after-install option and run section 8 before opening the desktop. For an unpublished candidate, record the native first launch directly.

| Format | What to exercise |
|---|---|
| Portable ZIP | Extract to a local path with spaces. Launch and relaunch `ExorSystem.exe` from Explorer. No installer or adjacent `data` directory should be created. Removing the extracted application must not remove its user profile. |
| NSIS EXE | Verify the visible install flow, Start-menu shortcut, actual binary launch, an upgrade from the previous version, and uninstall via Windows Settings. Capture installer dialogs and any exit errors. |
| MSI | Install via Explorer or `msiexec.exe /i "<verified-package.msi>" /L*v "<install.log>"`. Verify the shortcut and installed launch, upgrade from the previous version (with a different package version), and uninstall. Keep verbose MSI logs. Also verify the installer's launch-after-install option if offered. |

For each format:

1. Verify the desktop does **not** prompt for UAC and runs unelevated. Installing or removing a machine-wide package may prompt separately. If an installer run from an elevated terminal launches an elevated copy, confirm ExorSystem refuses to open and can be launched normally afterward. In a separate fresh-profile snapshot, deliberately launch as Administrator once: expect a clear refusal and no new profile or data writes. A second normal instance should report that the profile is in use.
2. Verify WebView2 loads the actual desktop, including on a Windows 10 VM without a preinstalled runtime. Note the runtime version and what installed it, or record the missing-runtime error. Record SmartScreen/signature behavior without turning off security controls.
3. In **Settings**, confirm the database and quarantine are under the normal account's `%LOCALAPPDATA%\ExorSystem`. Close and relaunch to check persistence. For the legacy-data test, make a disposable `data` directory beside a portable copy with a marker file. Confirm Settings mentions it, it is not read as the new profile, and its hashes are unchanged after restart. Legacy data is deliberately not auto-imported.
4. Test the upgrade from the previous stable build in a separate disposable snapshot. Create fixture History and a held batch in the older app, close it, and back up the **entire** Local AppData profile. Upgrade using the exact candidate in the same installer format, or extract the portable package to a new folder and open it against the fixture profile. Verify History and held files survive, including any migration. Exercise the candidate's documented rollback on a test copy, using the pre-upgrade full profile. Never open a newer schema in an older app; a SQLite-only migration backup does not cover held files. For the historical v0.2.0 → v0.3.0 exercise, see the [v0.3.0 rollback notes](releases/v0.3.0.md#before-you-upgrade).

## 3. Verify Watch and History with real files and registry keys

From a non-elevated PowerShell in the checkout on the VM:

```powershell
$fixture = ./scripts/windows-fixture.ps1 -Action New
$fixture | Format-List
```

The script prints an ID, file folders, exact registry subtrees, a per-user inventory name, and a portable executable path. It changes only its own files and HKCU keys; it does not install anything or alter a service, scheduled task, or driver.

1. In **Registry monitor → Watch an install**, set a distinct name ending in `A`, enter `$fixture.folder_a` in Folders and `$fixture.registry_a` in Registry subtrees. Leave Services and Scheduled tasks unchecked for this test. Select **Take baseline**. Verify File system and Registry baselines are ready and both system checks say **Not requested**, then run `./scripts/windows-fixture.ps1 -Action ChangeA -Id $fixture.id`. Switch to another tab and back, then select **Compare snapshots**. Verify File system and Registry were compared and choose **View recorded changes**.
2. The Change inspector’s Installation filter must show that Watch's session ID. Expect `a-created.txt`, `a-change.txt`, and removal of `a-gone.txt`; the registry `Added`, `Changed`, and removed `Gone` keys; and **Snapshot diff** as the source. A parent key may also change. No registry value data should appear. Focus should land on Change inspector. Confirm the split panes show registry and files separately, the file modification exposes the recorded before/after fingerprints, and Timeline still shows unassigned entries when the session filter is cleared. Trace one recorded file with a different ASCII case and verify it resolves to the recorded item.
3. Repeat with a different name ending in `B`, `$fixture.folder_b`, and `$fixture.registry_b` (initially missing). Between pictures run `./scripts/windows-fixture.ps1 -Action ChangeB -Id $fixture.id`. History for B must include the newly created Second key and B's files, not A's. Use **Older changes** and **Newer changes**; then search for `percent%_literal`, which should match exactly the fixture file with those literal characters. Switch to A, then to **All installations and unassigned changes** and verify the expected separation and combination of records. `-Action Verify -Id $fixture.id` confirms the input fixtures, not the desktop output.
4. Restart ExorSystem and reselect session A in **Change inspector**. Confirm the scope counts and skip details match the original comparison, even when a path filter matches no observations. Sessions from builds that did not save coverage should show it as unknown. In **Applications**, save the matching session’s report and check its coverage totals. Export filtered History as JSON and, if multiple parts are needed, check that the next older part reuses the filters without repeating rows.
5. Optional over-limit input: `./scripts/windows-fixture.ps1 -Action MakeOversized -Id $fixture.id`. Take before/after pictures of `$fixture.oversized`; expect an incomplete-scope warning, not thousands of changes. Test **Cancel watch** with a new Watch. In a **Thorough** cleanup preview, registry observations must never be selected for file quarantine.

If you lack a VM, a **file-only smoke test** on a normal Windows account can exercise Watch without touching installed apps. Create a new folder under `%TEMP%` with two disposable files. Enter only that folder, leave the registry and system checks off, take the baseline, then edit one file, add one and remove the other original. Compare and verify one Added, Changed and Removed entry and different before/after fingerprints for the edited file. Record the OS build, original package hash, and result as **partial smoke evidence**. Do not run the uninstall and cleanup drills on a live profile or report a file-only smoke as coverage of the full client matrix.

## 4. Verify inventory, cleanup and recovery

- Compare a classic entry and a current-user MSIX app with Windows Settings. MSIX and the fixture's HKCU program must have no ExorSystem uninstall action. The HKCU program must still support **Review leftovers → Find leftovers**. Add `$fixture.portable_exe` as a portable app, restart, forget the entry, and verify the executable's SHA-256 is unchanged. Do not run the fixture's uninstall entry from Windows Settings.
- Use an expendable machine-wide classic app for the positive uninstall case. ExorSystem must show the registered executable and arguments, then require confirmation before launching, not execute on selection. Check cancellation/nonzero exit and a successful uninstall if the program supports being launched without elevation. After its registry entry disappears, it must be review-only for the rest of the session. For an executable that requires elevation or a command format ExorSystem refuses, expect guidance to use Windows Settings, not a privileged retry. Record the chosen program/version and result; do not substitute a live user app.
- For a candidate with the uninstall review queue, select two expendable supported classic fixtures across inventory pages or filters. Confirm no process runs on selection or preview. Check one active vendor review, per-item exit/blocked status, and deliberate skip and fresh retry. Verify the next item and leftover review do not start automatically. Record both actual vendor outcome and observed registration independently.
- Select only the test folder in **Review leftovers → Find leftovers** for `$fixture.display_name`. Registry, service, and task suggestions are advisory, not selectable for removal. Record the SHA-256 of fixture A's files. Open **Review exact files…**, review every included page, and approve the exact count. Confirm the approved files move to quarantine. Recreate `a-change.txt` with different bytes, then attempt Restore. The new file must stay unchanged and the held batch must remain available. Remove only the fixture conflict and retry; restored SHA-256 values must match. Use another disposable batch to verify **Delete permanently** requires its final confirmation and retains a manifest.
- In a second terminal, run `./scripts/windows-fixture.ps1 -Action LockFile -Id $fixture.id`, then try quarantining the A folder. Expect failure without losing either the locked file or any earlier file. Release the lock and verify originals before retrying. For a junction drill, make a junction inside A targeting B (both fixture paths); confirm B is unaffected by quarantining A. Remove only the junction, or revert the snapshot. Do not point it at system paths.
- Run `cargo test -p exor-engine --locked` on disposable Windows if you need to repeat the database write, sharing violation, reparse, and interrupted-batch drills. The tests inject record/ledger failures in their own databases. They do not tamper with the desktop's profile. For a different-volume case, use only expendable fixture volumes; expect refusal rather than a cross-volume copy, with the original or held files unchanged. Real shutdown and long-path behavior remain separate native checks. Record skipped cases explicitly.

Close ExorSystem, finish or restore its fixture batches and forget the portable entry before removing the fixture. Use `./scripts/windows-fixture.ps1 -Action Remove -Id $fixture.id` or revert the VM snapshot. The script does not recursively remove fixture file folders or touch the desktop database.

## 5. Accessibility and native client evidence

### Docking and accessibility, on each Windows client

Use the exact verified package and a normal test profile. See the [workspace controls](DASHBOARD.md#arrange-the-workspace).

1. Apply **Investigation layout**. Verify Main, Right dock, and Bottom dock each show the selected panel and scroll independently. Drag a tab into another dock, including one that is empty, and reorder tabs. Dropping a disposable Explorer file onto the workspace should not open it or change the layout. Move the same panel back using only Tab/Shift+Tab and **Dock [panel name]** → destination. Resize both splits by mouse and keyboard; confirm focus lands on the moved tab and remains usable after **Close** or **Delete**. Reopen a closed tool via the sidebar and check it returns to its previous dock.
2. With the file-only Watch fixture, take a baseline. Move or close and reopen **Registry monitor** before finishing. Confirm there is still one baseline and no extra Watch, then compare. Move the panel while the comparison is running if it takes long enough. From **View recorded changes**, check that an already-open Change inspector switches to the correct session and receives focus. Move a Scan folder panel with a draft or an active scan and verify progress/result where observable. Review a supported **Script preview** from Applications when the preview is already docked; no command launches. Open the wizard without running anything and check that workspace shortcuts do not bypass its modal review.
3. Resize or snap the window until the dock switcher appears. Verify all three docks are reachable and no controls are clipped; expand to restore split widths. Close and launch from the same normal profile and verify placement, order, active tabs and splitter sizes persist. Apply **Cleanup layout** and **Reset layout**; verify history and batches are unchanged. An applied theme should persist, an unapplied preview should end when hidden, and **Undo** should restore the previous theme.
4. Open **Activity log** from the sidebar, **Log**, and **Ctrl+`**. Confirm it is a separate window with no duplicate dock tab. On a normal window, move and resize, including near the monitor’s bottom edge; it should stay below the main window and clear the taskbar. Minimize/restore both. Maximize with the log open and also open it while already maximized. In each case close the log (button and **Alt+F4**) and verify the main window fills the work area, remains maximized, and can still restore to its normal size. Move across two monitors at different scaling settings. If the VM cannot exercise mixed-monitor DPI, mark that check Blocked, not Pass. In Applications, select a row near the bottom of a page with long registration or evidence; scroll to the last action and use **Back to applications** in a narrow dock. Clear the log and verify the status bar resets without affecting Change inspector; then produce another disposable scan event. Record any scenarios not run.
5. Use Narrator and Windows high contrast; verify tool, dock, tab, separator, and modal names, keyboard focus, panel transitions, and Watch status. Check 100% and 150% display scaling, including the minimum window size. Record actual OS and WebView2 versions, scale and monitor setup. Use fixture paths and redact personal data in screenshots (see the [capture list](SCREENSHOTS.md)).

Use the [evidence template](WINDOWS-RELEASE-EVIDENCE.md) to record the commit, per-format hash, OS build, VM setup, WebView2 version, status, and logs. Mark anything not run **Not run**, not Pass. A reviewed, complete candidate must also have a [machine-checked sign-off record](release-evidence/README.md) before publication. The [live-source/driver gates](LIVE-COLLECTION.md) are separate and do not block driverless desktop packaging.

## 6. Tag and publish the tested candidate

Use the separate **Release** workflow to publish. Candidate building and static verification still run without client evidence; promotion does not.

1. Pick a successful **Windows candidate packages (no publish)** run from this repository with a retained artifact. Its exact source commit must be reachable from `main` and have passing CI. If a squash merge made the candidate commit unreachable, build and test a candidate on the landed commit. If the artifact expires or the bytes change, use the new run and test its exact packages.
2. Test each package on both disposable Windows 10 and 11 x64 clients using sections 2–5. A maintainer must review the observed results and land [the sign-off for that run](release-evidence/README.md) on the default branch. Missing, failed, blocked, or not-run required cases prevent promotion. Windows Server and the optional launch-only observer do not meet the gate. Complete the sign-off while the candidate artifact is still retained (30 days by default).
3. In Actions → **Release** → **Run workflow**, select the default branch after sign-off lands and enter `candidate_run_id` (the number in the run URL). Do not create the tag first or use this workflow just to build packages.
4. The workflow binds the reviewed client matrix to the exact candidate version, source commit, run ID and SHA-256 of each package. It refuses to reuse a tag, then publishes the **same bytes**, checksums, and package report from the candidate. Release notes link CI, static checks, and the reviewed interactive matrix separately. Check the tag target and assets afterward. VM automation and release signing remain [Phase 1 work](../ROADMAP.md#phase-1--uninstall-credibility-and-release-trust-03-months).

## 7. Verify the published downloads without launching them

**Published Windows package evidence (no launch)** is a read-only Actions workflow. It checks v0.3.0 on related PRs; for a repeat run, select the workflow on the default branch and enter an exact stable tag. It downloads the release assets, not a fresh build or the source archive. The job attaches `packages.json` as **ExorSystem-published-package-evidence** when the download or verification step runs. Keep that report with any later client evidence. A failed download is **Blocked**, not a pass. This workflow does not gate or run the Release workflow. See the [passing v0.3.0 run](https://github.com/SeanShaffer88/ExorSystem/actions/runs/44434816182) for the actual published-assets result.

For a local run, use Python 3.11+ and GitHub CLI (`gh`). The example uses PowerShell 7.2+ from the repository checkout. Fetch release JSON directly from the [official repository](https://github.com/SeanShaffer88/ExorSystem); a JSON file supplied by the same mirror as the downloads is not a trust anchor. Use a new local staging directory. If downloading via the release page instead of `gh release download`, put **only** the NSIS setup EXE, MSI, portable ZIP, `SHA256SUMS.txt`, and `PACKAGE-CHECKS.json` in `packages`.

```powershell
$repo = 'SeanShaffer88/ExorSystem'
$tag = 'v0.3.0'
$stage = Join-Path ([IO.Path]::GetTempPath()) ('exor-release-' + [guid]::NewGuid().ToString('N'))
$packages = Join-Path $stage 'packages'
$info = Join-Path $stage 'release.json'
New-Item -ItemType Directory -Path $packages | Out-Null
$release = gh api "repos/$repo/releases/tags/$tag"
if ($LASTEXITCODE) { throw 'Could not fetch official release metadata.' }
$ref = gh api "repos/$repo/git/ref/tags/$tag" --jq '.object.type + ":" + .object.sha'
if ($LASTEXITCODE -or $ref -cne ('commit:' + ($release | ConvertFrom-Json).target_commitish)) {
  throw 'The release source does not match the published tag.'
}
$release | Set-Content -LiteralPath $info -Encoding utf8
gh release download $tag --repo $repo --dir $packages
if ($LASTEXITCODE) { throw 'Could not download the five release assets.' }
python -B ./scripts/verify-published-release.py $packages --release-info $info --tag $tag --output (Join-Path $stage 'packages.json')
if ($LASTEXITCODE) { throw 'Stop: published package verification did not pass.' }
```

The verifier compares all five file sizes and SHA-256 digests with GitHub release metadata. It checks consistency with the candidate report, release notes, and published checksum file; inspects the NSIS launcher PE header, MSI compound-file header, and expected portable ZIP entries; and fingerprints the x64 desktop executable inside the ZIP. The candidate report is the evidence for the MSI x64 template and embedded manifest; this post-publication check does not rerun those SDK checks. It creates a new report without extracting, installing, launching, signing, or modifying any package. It requires the five-asset release layout used by the retained-candidate workflow; older releases without this metadata cannot pass.

## 8. Observe first launch and exit on a disposable client

This is an **optional, separate** smoke test for one OS/format pair. It needs an interactive Windows 10 or 11 x64 client VM, a normal unelevated account with **no existing** `%LOCALAPPDATA%\ExorSystem` profile, x64 PowerShell 7.2+, and Python 3.11+. Start from a clean snapshot, verify the downloads as in section 7, then manually install **one** verified installer or extract the verified ZIP through Explorer. Do not start ExorSystem yet; skip any installer option to launch it. Leave SmartScreen, UAC, and other security controls in place. Use ordinary local paths without junctions or symlinks.

From that same account, with the section 7 variables still set:

```powershell
$exe = 'C:\Test Apps\ExorSystem\ExorSystem.exe' # Replace with the actual extracted desktop path.
./scripts/windows-client-smoke.ps1 -ArtifactDir $packages -ReleaseInfo $info -Tag $tag `
  -EvidenceDir (Join-Path $stage 'portable-launch') -Format Portable -DesktopExe $exe -DisposableClient
```

For an installer, use `-Format NSIS` or `-Format MSI` and the installed desktop `.exe` (not its shortcut, setup program, or uninstaller). Choose a new evidence directory each time. The observer rechecks the five downloads and requires the desktop executable to match the portable binary's size and SHA-256. When prompted, open that exact file normally from Explorer in the test session, then close its window when asked. The observer does not start, stop, or kill processes itself. It refuses Windows Server, elevated shells, and pre-existing profiles; it never deletes data or redirects the real Local AppData known folder.

| Report | What a pass means |
|---|---|
| `packages.json` | The downloads match the release and pass the static checks in section 7. No client results. |
| `launch.json` | After verifying the binary, the observer saw a new responsive window titled **ExorSystem** from that path in the test session, a new nonempty `exor.db` in the test profile, and process exit code `0` within the close timeout. |

`-Format` records the operator's selection; it does not prove which installer registered the executable. A responsive window and exit code are not a WebView2 render test or a record of the close click. If a guard refuses the run, security or a missing runtime blocks launch, or an observation times out, record the actual outcome and reason in the [client evidence template](WINDOWS-RELEASE-EVIDENCE.md). A refused run may produce no `launch.json`. If the desktop stays open, close it normally or revert the VM after saving the evidence. Do not delete a profile to force a fresh run. Use sections 2–5 for actual install/upgrade/removal, WebView2 and IPC, Watch/History/cleanup, and accessibility/DPI results; leave unobserved checks **Not run**.
