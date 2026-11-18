# Windows release evidence template

Copy for each candidate. Attach `PACKAGE-CHECKS.json`, `SHA256SUMS.txt`, and relevant logs to the PR. Avoid personal file paths, account identifiers, and registry value data in screenshots or logs.

- Source commit / app version:
- Passing CI run / successful Windows candidate Actions run ID and link:
- Tester / UTC date:
- Windows 10 x64 edition, build and VM snapshot:
- Windows 11 x64 edition, build and VM snapshot:
- Account type, WebView2 versions and display setup:

Use **Pass**, **Fail**, **Blocked**, or **Not run**. Add exact SHA-256 and evidence links for each package test. Include install/launch/remove and applicable upgrade outcomes; attach the verbose MSI log where available. The template itself is not evidence.

| OS | Format | SHA-256 | Install/extract, launch, upgrade, uninstall | UAC, WebView2, profile | Status / evidence |
|---|---|---|---|---|---|
| Windows 10 | NSIS | | | | Not run |
| Windows 10 | MSI | | | | Not run |
| Windows 10 | Portable ZIP | | | | Not run |
| Windows 11 | NSIS | | | | Not run |
| Windows 11 | MSI | | | | Not run |
| Windows 11 | Portable ZIP | | | | Not run |

Record the format used for each functional run. Repeat it for another format if install/launch behavior differs.

| Test | Windows 10 status / evidence | Windows 11 status / evidence |
|---|---|---|
| Non-elevated launch, elevated refusal and profile lock | Not run | Not run |
| Classic/MSIX/portable inventory, scope and persistence | Not run | Not run |
| Classic uninstall, fallback and leftover review | Not run | Not run |
| Watch → session Change inspector / History: selected/captured/skipped scopes, registry/file split, A/B isolation, warnings, removals, hashes, pages, literal filter | Not run | Not run |
| Case-insensitive trace, relationship links, and registry/service/task review only | Not run | Not run |
| Quarantine, conflicting destination, restore hash, purge confirmation, and storage totals | Not run | Not run |
| Sharing violation, junction, cross-volume NTFS and long paths | Not run | Not run |
| Interrupted-batch/restart and legacy data preservation | Not run | Not run |
| Docking: drag and keyboard move/reorder, close/reopen, focus recovery, both splitters | Not run | Not run |
| Compact dock switcher, restart persistence, layout presets/reset and theme preview/Undo | Not run | Not run |
| Active Watch/Scan retained; context links update existing inspector and Script preview; modal shortcuts blocked | Not run | Not run |
| Activity log: shortcut, shared messages/clear, attached log follow, minimize and reduced motion | Not run | Not run |
| Keyboard, Narrator, high contrast and 100%/150% DPI (including second monitor) | Not run | Not run |

- Failed/skipped cases, issue links, decisions and re-test evidence:
- Sign-off decision / operator / date (do not infer Pass):

A passing package build, Windows Server test, or browser demo does not fill a client row. A partial smoke on a live PC is helpful but not a substitute for the disposable Windows 10 and 11 runs. Leave the release on hold if either client matrix has required cases that are failing, blocked, or not run.
