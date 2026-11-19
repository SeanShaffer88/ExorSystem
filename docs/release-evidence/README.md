# Native Windows client release sign-off

The Release workflow requires a **reviewed sign-off for the exact retained candidate**. This directory holds those records once real runs pass. There is no template or default pass file that the workflow can use. The already-published v0.3.0 result remains [Not run](../../AUDIT.md#v030-release-evidence).

## Collect and review a candidate's evidence

1. Download the retained NSIS, MSI, portable ZIP and both evidence files from one successful **Windows candidate packages (no publish)** run with passing CI. Verify the package hashes before testing. Do not substitute locally built packages or the release's source archive.
2. Follow [Windows runbook sections 2–5](../WINDOWS-RELEASE.md#2-install-start-and-remove-each-format) on disposable **Windows 10 x64 and Windows 11 x64** client VMs. Restore the appropriate clean snapshot between formats. Use the [working test matrix](../WINDOWS-RELEASE-EVIDENCE.md) and save real, redacted evidence for each of the six OS/package pairs. A launch-only observation, mocked browser, and Windows Server result are not substitutes.
3. Have a maintainer review the results and hashes before landing `docs/release-evidence/<candidate-run-id>.json` on the default branch. The record can land after the candidate build; the candidate source still needs to be an ancestor of the release workflow's default-branch checkout. Use a reviewable PR and keep original partial, blocked, and failed results with the evidence. Add a passing sign-off only after actual passes. Prefer a reviewer other than the tester.
4. Dispatch **Release** with that candidate run ID, after the reviewed record lands. The workflow reads its policy and sign-off from the default-branch dispatch commit, not from the downloaded artifact or the candidate checkout. It adds the verified summary and an immutable link to the detailed record to the notes without rebuilding or changing the five release assets.

The script checks the record structure, required cases and exact hash bindings of the attestation. It **does not** connect to the linked logs, prove the identity of the tester or reviewer, or run Windows. The maintainer must check the links, OS and actual results. Disposable-client automation is still planned; do not run a publishing workflow or hand-write a Pass to test the gate.

### Capture the client build and package hash

On the test VM, use the observed build plus revision and the native OS architecture (not the architecture of PowerShell). Compare `PackageSha256` with the exact candidate's checksum before testing. Set `$package` to the path of the one verified package under test.

```powershell
$os = Get-CimInstance Win32_OperatingSystem
$nt = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
[pscustomobject]@{
  Build = "$($os.BuildNumber).$($nt.UBR)"
  Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
  ProductType = $os.ProductType
  PackageSha256 = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
}
```

Record the WebView2 runtime version actually used when the installed desktop renders. A build number or hash by itself is not a test result.

## Record contract (schema 1)

Use exactly these top-level fields: `schema_version` (integer `1`), `kind` (`native-windows-client-signoff`), `app_version` (e.g. `0.4.0`, no `v`), `commit` (full candidate source SHA), `candidate_run` (Actions run ID as a string), `reviewed_by` (GitHub handle), and `runs` (six objects). Copy `app_version` and `commit` from the exact candidate's `PACKAGE-CHECKS.json`; its `workflow_run` is the `candidate_run`. Each package basename must occur once for each client OS.

Each run has exactly these fields:

| Field | Required value |
|---|---|
| `os` | `windows-10` or `windows-11` |
| `build` | Observed build and update revision as a string, e.g. `19045.6000` |
| `arch` / `product_type` | `x64` / `1` (integer workstation product type, not Server) |
| `package` / `sha256` | Exact versioned basename / lowercase SHA-256 of the file tested, matching the retained candidate |
| `webview2` | Observed four-part runtime version, not just a responsive native window |
| `tester` / `tested_at_utc` | GitHub handle / UTC timestamp `YYYY-MM-DDTHH:MM:SSZ` |
| `evidence_url` | Durable HTTPS link to redacted results for this run; no credentials, query parameters, or access tokens |
| `checks` | Object with exactly the check IDs below. Only `pass` lets promotion proceed. Use `fail`, `blocked`, or `not_run` for other outcomes; don't omit cases. |

| Check ID | What the evidence must demonstrate |
|---|---|
| `install_or_extract` | The tested format installs or extracts properly and reaches the expected launch path. |
| `launch_and_exit` | The exact desktop renders and exits normally under a standard account. |
| `upgrade_profile_and_rollback` | Upgrade from the previous stable build and preserve the existing profile, including held files. Exercise documented backup/rollback on a test copy. For portable, open the new extraction against the fixture profile. |
| `removal` | Windows Settings removal or deletion of the portable extraction leaves the documented profile data intact. |
| `unelevated_profile_and_uac` | Correct per-user location, normal launch, already-elevated refusal and expected UAC behavior for the format. |
| `native_webview2_and_ipc` | Actual desktop rendering and native allow/deny behavior from the main and log windows, including the relevant native dialogs, Watch channels, and serial vendor review when present in the candidate. |
| `watch_and_inspector` | A fixture Watch commits honest coverage and observations; Change inspector shows the right session and data. Include older archive navigation when present in the candidate. |
| `cleanup_and_restore` | Review an exact fixture file plan, quarantine, refuse an overwrite, and verify the restored bytes. Exercise the [cleanup runbook](../WINDOWS-RELEASE.md#4-verify-inventory-cleanup-and-recovery). |
| `accessibility_and_dpi` | Keyboard, Narrator, high contrast and mixed-monitor 100%/150% DPI with the [native accessibility checklist](../WINDOWS-RELEASE.md#5-accessibility-and-native-client-evidence). |

## Local preflight (no publish)

From the release-policy checkout, use Python 3.11+ with the downloaded artifact directory and clean source checkout of the tested candidate. Replace the placeholders with actual values. Without `--client-evidence`, this script only validates package hashes for candidate CI; that mode is not a promotion check.

```sh
python3 scripts/verify-release-candidate.py /path/to/retained-candidate \
  --source-root /path/to/candidate-source --commit FULL_SOURCE_SHA --run-id RUN_ID \
  --client-evidence docs/release-evidence/RUN_ID.json --client-matrix-out /path/to/new-matrix.md
```
