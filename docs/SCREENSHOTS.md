# Windows screenshot checklist

Capture the installed or extracted desktop on a disposable Windows 10/11 x64 client using verified [release downloads](WINDOWS-RELEASE.md#7-verify-the-published-downloads-without-launching-them). Use the [Windows fixture](WINDOWS-RELEASE.md#3-verify-watch-and-history-with-real-files-and-registry-keys), not real app or user data. These filenames are a capture plan, not bundled images or test results.

| File | What to show |
|---|---|
| `overview.png` | Overview with inventory, recent observations, and recorded storage totals. |
| `applications.png` | Applications details and the uninstall wizard's read-only plan, using a disposable app. |
| `inspector.png` | Watch coverage, separate registry and file panes, and before/after detail on a changed file. |
| `relationships.png` | A recorded artifact with an origin or dependency, including its evidence label. |
| `cleanup.png` | Exact-file review or a held/restored batch, with recorded sizes distinguished from freed disk space. |
| `docks.png` | Investigation layout with Watch and Change inspector, and the Windows Activity log below the main window. Capture the compact dock switcher separately. |

Check 100% and 150% scaling and both monitors when available. For evidence, record the package hash, OS build, WebView2 version, and display setup in the [client template](WINDOWS-RELEASE-EVIDENCE.md). Redact personal paths, account identifiers, and unrelated installed apps. A screenshot is not a test of keyboard navigation, Narrator, or installation; label browser demo captures as demo rather than native release evidence.
