# Windows client evidence template

Copy this working template for interactive runs from the [Windows runbook](WINDOWS-RELEASE.md). Attach `PACKAGE-CHECKS.json`, `SHA256SUMS.txt`, and relevant redacted logs. Use a separate record for each target build; keep package and OS differences visible. Avoid personal file paths, account identifiers, and registry value data in reports or screenshots.

**Starting point:** [v0.3.0 published-package verification passed](../AUDIT.md#v030-release-evidence). The interactive result cells below are intentionally **Not run**.

For a published build, link `packages.json` from [static published package verification](WINDOWS-RELEASE.md#7-verify-the-published-downloads-without-launching-them) separately from any `launch.json` from the [client launch observer](WINDOWS-RELEASE.md#8-observe-first-launch-and-exit-on-a-disposable-client). A static pass fills no client cells. A launch-observer pass fills only the launch/exit cell for that OS and format; it does not attest installer actions, SQLite integrity, UAC, or rendered WebView2 content.

- Published release tag / static report or Actions run:
- Source commit / app version:
- Passing CI run / successful Windows candidate Actions run ID and link:
- Tester / UTC date:
- Previous package version and pre-upgrade profile snapshot, if testing upgrade:
- Windows 10 x64 edition, build and VM snapshot:
- Windows 11 x64 edition, build and VM snapshot:
- Account type, WebView2 versions and display setup:

Use **Pass**, **Fail**, **Blocked**, or **Not run**. Add exact SHA-256 and evidence links for each package test. Include install/launch/remove and upgrade outcomes (including portable profile compatibility); attach the verbose MSI log where available. The template itself is not evidence.

| OS | Format | SHA-256 | First launch / exit | Install or extract / removal | Upgrade | UAC, WebView2, profile |
|---|---|---|---|---|---|---|
| Windows 10 | NSIS | | Not run | Not run | Not run | Not run |
| Windows 10 | MSI | | Not run | Not run | Not run | Not run |
| Windows 10 | Portable ZIP | | Not run | Not run | Not run | Not run |
| Windows 11 | NSIS | | Not run | Not run | Not run | Not run |
| Windows 11 | MSI | | Not run | Not run | Not run | Not run |
| Windows 11 | Portable ZIP | | Not run | Not run | Not run | Not run |

Use one copy of the functional checklist per OS/format pair and note the package SHA-256 in that copy. Map the six sets of results to the [required check groups](release-evidence/README.md#record-contract-schema-1) for sign-off.

- Functional checklist: OS edition/build, format, SHA-256, WebView2, VM snapshot, tester, UTC:

| Test | Status | Redacted evidence / notes |
|---|---|---|
| Non-elevated launch, elevated refusal and profile lock | Not run | |
| Classic/MSIX/portable inventory, scope and persistence | Not run | |
| Classic uninstall, fallback and leftover review | Not run | |
| Serial uninstall queue, when present: multi-select, separate confirmation, skip, live retry and no automatic cleanup | Not run | |
| Watch → session Change inspector / History: selected/captured/skipped scopes, registry/file split, A/B isolation, warnings, removals, hashes, pages, literal filter | Not run | |
| Case-insensitive trace, relationship links, and registry/service/task review only | Not run | |
| Quarantine, conflicting destination, restore hash, purge confirmation, and storage totals | Not run | |
| Exact cleanup approval: explicit link, cancel, all pages, replay, stale registration and replaced file | Not run | |
| Sharing violation, junction, named streams, cross-volume refusal and long paths | Not run | |
| Interrupted operation/restart, DB write failure and v4 → v5 data preservation | Not run | |
| Verified backup and separate restore, bad archive rejection and history-preserving rotation | Not run | |
| Packaged IPC allow/deny from main and log, capture Channels, native pickers and reviewed argv | Not run | |
| UI under bounded concurrent work, close during capture/apply/vendor process, UTC export continuation | Not run | |
| Docking: drag and keyboard move/reorder, close/reopen, focus recovery, both splitters | Not run | |
| Compact dock switcher, restart persistence, layout presets/reset and theme preview/Undo | Not run | |
| Active Watch/Scan retained; context links update existing inspector and Script preview; modal shortcuts blocked | Not run | |
| Applications: long selected details, short and narrow docks, Back preserves row and focus | Not run | |
| Activity log: owned bottom attachment, shortcuts, clear, move/resize/monitor, minimize/restore, reclaim work area while maximized | Not run | |
| Keyboard, Narrator, high contrast and 100%/150% DPI (including second monitor) | Not run | |

- Failed/skipped cases, issue links, decisions and re-test evidence:
- Overall client test result / tester / date (do not infer Pass):

A passing package build, Windows Server test, or browser demo does not fill a client row. A partial smoke on a live PC is helpful but not a substitute for the disposable Windows 10 and 11 runs. This Markdown template is a working record, not the machine-checked [release sign-off](release-evidence/README.md). Keep failures and cases that were not run visible; automated package checks do not replace them.
