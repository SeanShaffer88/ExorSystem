# Desktop dashboard

ExorSystem is a local evidence and cleanup workspace. The sidebar groups the tools by task; **Overview** shows installed inventory, session counts, recent observations, and cleanup outcomes. The dashboard does not need the driver or an elevated process.

## Arrange the workspace

- Open a tool from the sidebar. Each tool is a single panel that can live in **Main**, **Right dock**, or **Bottom dock**. Drag its tab to another dock, or open the **Dock [panel name]** button beside its tabs and choose a destination. Empty right and bottom drop targets appear while dragging. Drag onto a tab to insert before it.
- Drag the separator to size a split. For keyboard use, focus the separator and press its arrow keys; **Home** and **End** set its smallest and largest sizes. Narrow or short windows show a dock switcher instead of piling up small splits. Growing the window restores the saved split sizes.
- In the sidebar, use arrow keys or **Home/End** to switch tools. In a dock's tab strip, use **Left/Right** or **Home/End** to switch tabs, **Ctrl+Left/Right** to reorder, and **Delete** to close the selected tab. You can also use that dock's **Close [panel name]** button. Open a closed tool from the sidebar to return it to its last dock.
- Use **Layout** in the workspace header for **Investigation layout** (inspector, Watch and Activity log), **Cleanup layout** (batches, dry run and Activity log), or **Reset layout**. Presets do not start work or change your records. Tab placement, selection, and sizes are saved locally. Form drafts, active operations, and scroll positions live only in the current app session; closing or moving a tab does not cancel a Watch or Scan. Use the operation's own cancel control.
- Open **Activity log** from the sidebar or toggle it with the status bar's **Log** button or **Ctrl+`**. It shares scrollback with the optional **Open attached window** on the Windows desktop. Make room to the right if using the attached window. **Clear log** clears session messages in both places, not Change inspector or recorded evidence. **Script preview** stays separate and read-only.

## Tools

| View | What it shows | Where the data comes from |
|---|---|---|
| **Applications** | Search by name, publisher, or version. Filter classic, MSIX, portable, and recorded entries. Select an app to see its registration, linked sessions, and supported actions. | Windows uninstall registrations, current-user packages, opted-in portable entries, and local session records. Installation sizes are Windows estimates when present. |
| **Registry monitor** | Choose snapshot scopes, check the baseline, and see which sources were compared or skipped. Browse recently observed registry keys. | Explicit file folders and Software subtrees; services and scheduled-task metadata are separately opt-in. No live process attribution or registry value capture. |
| **Change inspector** | Side-by-side registry and file lists with session and literal path filters. Select a change for before/after details, or use **Timeline** for all observations including services and tasks. | Paged SQLite ledger. File details show recorded fingerprints, not contents. Registry snapshots show key presence or a last-write change, not the raw timestamp or the values. A missing fingerprint is not proof of absence. |
| **Relationships** | Trace a recorded path and follow links to its origins, dependencies, and dependents or shared nodes. Select a node to trace it in turn. | A bounded, one-hop view of recorded relationships with evidence sources and confidence, not an exhaustive Windows dependency analysis. |
| **Activity log** | Recent messages with time and severity; scroll up to review older messages. | The desktop's in-memory log for this run, not the durable SQLite ledger. |
| **Script preview** | Inspect the executable path and arguments of a supported, currently registered vendor uninstaller. | The same live command resolution the wizard uses. This is a read-only terminal-style panel, not a shell or script runner. |
| **Cleanup summary** | Compare the recorded sizes of purged, held, and restored files. Review batches, restore held files, or explicitly confirm permanent deletion. | Logical file lengths from quarantine manifests. These are not measured disk allocation or free-space gains; held files still take space. |
| **Policy simulator** | Dry-run the policy against a recorded installation. Green identifies candidates for reversible quarantine, red identifies protected items to keep, and amber identifies review-only items. | Engine decisions for the chosen policy. No badge authorizes deletion; change badges in the inspector describe observations, not risk. |

## Watch an install

1. In **Registry monitor**, enter an application or test name. Add full folder paths, specific registry subtrees, or both. No folders are selected for you. Leave the optional installer path blank for a test; ExorSystem never launches an installer from Watch.
2. Leave **Services** and **Scheduled tasks** unchecked for file- or registry-only work. If you opt in, these are read-only checks. Windows can deny access; do not run ExorSystem as Administrator to retry.
3. Choose **Take baseline**. The coverage cards identify captured baselines and excluded scopes. If some were skipped, expand **Skipped scope details**; continue with the usable baselines, or cancel and choose narrower scopes. Then make your test changes or run your installer yourself.
4. Choose **Compare snapshots**. **Not requested** and **Skipped** have no change count. A scope that was actually compared can report zero. **Partial** means only some of the selected folders or subtrees were comparable. **View recorded changes** opens that session in Change inspector. Use **Set up another watch** to reuse or edit your input.

A missing folder or registry subtree can appear or disappear between snapshots. A folder with reported read or hash failures, an unreadable task definition, or an incomplete registry subtree is excluded rather than diffed against an incomplete listing. The coverage report is part of this Watch result; the ledger stores observations, not complete scope coverage. **Compared** means a usable snapshot pair for the selected source, not proof that nothing else changed on Windows.

## Uninstall wizard

1. **Review plan.** Select a supported machine-wide classic app in Applications and open **Uninstall wizard**. Check the resolved executable and its separate arguments. Use Windows Settings for per-user or unsupported programs; MSIX and portable entries have no local uninstall action.
2. **Vendor uninstall.** Confirm before launching. The desktop refuses a changed registration and does not fall back to a shell or elevation. While it waits, the progress indicator is indeterminate; a return code is not proof the app disappeared.
3. **Review leftovers.** Run the read-only check separately. Review warnings and select folders yourself, then confirm any file move. A matching folder is not proof of ownership. Registry keys, services, and scheduled tasks are never moved or changed. You can open this review without running a vendor uninstaller.
4. **Summary.** See what ran and whether a quarantine batch was created. Moving files does not permanently delete them. Confirm any permanent deletion separately in Cleanup summary.

Press Escape to close the wizard when it is not busy. Workspace shortcuts do not switch panels while the modal wizard is open.

## Browser preview

Run `npm ci` and `npm run dev` from `gui/` and open the Vite page in a regular browser. The **Demo data** badge identifies an in-memory fixture; actions there never launch Windows processes or inspect or change the host. The fixture is not included in production bundles or used by Tauri. Browser tests use a separate mocked IPC fixture to check the built interface. Use the [Windows release runbook](WINDOWS-RELEASE.md) for optional native tests on exact packages.
