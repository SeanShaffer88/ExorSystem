# Desktop dashboard

ExorSystem is a local evidence and cleanup workspace. The sidebar groups the tools by task; **Overview** shows installed inventory, session counts, recent observations, and cleanup outcomes. The dashboard does not need the driver or an elevated process.

| View | What it shows | Where the data comes from |
|---|---|---|
| **Applications** | Search by name, publisher, or version. Filter classic, MSIX, portable, and recorded entries. Select an app to see its registration, linked sessions, and supported actions. | Windows uninstall registrations, current-user packages, opted-in portable entries, and local session records. Installation sizes are Windows estimates when present. |
| **Registry monitor** | Set up a before/after watch and follow baseline, installer, and comparison steps. Browse recently observed registry keys. | Explicit Software subtrees and file folders; there is no live process attribution or registry value capture in this desktop. |
| **Change inspector** | Side-by-side registry and file lists with session and literal path filters. Select a change for before/after details, or use **Timeline** for all observations including services and tasks. | Paged SQLite ledger. File details show recorded fingerprints, not contents. Registry snapshots show key presence or a last-write change, not the raw timestamp or the values. A missing fingerprint is not proof of absence. |
| **Relationships** | Trace a recorded path and follow links to its origins, dependencies, and dependents or shared nodes. Select a node to trace it in turn. | A bounded, one-hop view of recorded relationships with evidence sources and confidence, not an exhaustive Windows dependency analysis. |
| **Script preview** | Inspect the executable path and arguments of a supported, currently registered vendor uninstaller. | The same live command resolution the wizard uses. This is a read-only terminal-style panel, not a shell or script runner. |
| **Cleanup summary** | Compare the recorded sizes of purged, held, and restored files. Review batches, restore held files, or explicitly confirm permanent deletion. | Logical file lengths from quarantine manifests. These are not measured disk allocation or free-space gains; held files still take space. |
| **Policy simulator** | Dry-run the policy against a recorded installation. Green identifies candidates for reversible quarantine, red identifies protected items to keep, and amber identifies review-only items. | Engine decisions for the chosen policy. No badge authorizes deletion; change badges in the inspector describe observations, not risk. |

## Uninstall wizard

1. **Review plan.** Select a supported machine-wide classic app in Applications and open **Uninstall wizard**. Check the resolved executable and its separate arguments. Use Windows Settings for per-user or unsupported programs; MSIX and portable entries have no local uninstall action.
2. **Vendor uninstall.** Confirm before launching. The desktop refuses a changed registration and does not fall back to a shell or elevation. While it waits, the progress indicator is indeterminate; a return code is not proof the app disappeared.
3. **Review leftovers.** Run the read-only check separately. Review warnings and select folders yourself, then confirm any file move. A matching folder is not proof of ownership. Registry keys, services, and scheduled tasks are never moved or changed. You can open this review without running a vendor uninstaller.
4. **Summary.** See what ran and whether a quarantine batch was created. Moving files does not permanently delete them. Confirm any permanent deletion separately in Cleanup summary.

Use arrow keys and Home/End in the sidebar, Tab to enter the view, and Escape to close the wizard when it is not busy. The existing docked activity log remains separate from Script preview.

## Browser preview

Run `npm ci` and `npm run dev` from `gui/` and open the Vite page in a regular browser. The **Demo data** badge identifies an in-memory fixture; actions there never launch Windows processes or inspect or change the host. The fixture is not included in production bundles or used by Tauri. Browser tests use a separate mocked IPC fixture to check the built interface. Use the [Windows release runbook](WINDOWS-RELEASE.md) for native sign-off on exact packages.
