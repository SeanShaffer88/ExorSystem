# Architecture

```
GUI (Tauri v2 + React)  --invoke-->  exorsystem-gui (src-tauri)
                                          |
                    +---------------------+---------------------+
                    v                                           v
            exor-engine (Rust, tokio, safe)             exor-storage (SQLite WAL)
   walker/trie/snapshot  noise  policy  quarantine      schema, migrations, DAG, ledger
   monitor  fingerprint  risk  portable  report
                    ^
                    | EventSource trait (mpsc<RawEvent>)
        +-----------+--------------+
        |                          |
  exor-win (Windows)          MockEventSource (tests)
  USN journal reader          seeded generators
  registry driver client
        ^
        | \\ExorSystemPort (packed messages, parsed + bounds-checked in Rust)
  driver/exordrv.c  CmRegisterCallbackEx (observe-only)
```

## Decisions
1. **Edge direction**: an edge reads `child <relationship> parent` (artifact `created_by` installer; app `depends_on` library). `parent_node_id` is the origin or dependency.
2. **Schema**: `0001` is the blueprint schema verbatim. The two PRAGMAs run per connection (they cannot run inside a migration transaction). `0002` adds the change ledger, quarantine records and a unique edge index, which the blueprint's pillars require but its schema lacks. `0003` adds opt-in portable inventory and case-insensitive read indexes without changing node identity.
3. **Confidence ladder**: manifest 1.0, temporal 0.7, heuristic 0.3. Re-adding an edge keeps the highest score, so evidence only ever raises confidence.
4. **Quarantine safety**: write an `in_progress` manifest before moving anything; persist the record and ledger in a single SQLite transaction, then finalize. On startup, a matching committed record finishes the manifest; otherwise verified, unblocked blobs go back to their original paths. A conflict keeps its blob and produces a warning. Restore/purge derive paths from the configured root and ID, do not use SQLite as path authority, and never overwrite an original.
5. **Policy precedence**: when several rules match, the most protective wins (keep, review, quarantine). No match means review; non-file artifacts cannot be quarantined.
6. **Case-insensitive paths** in the trie (Windows semantics on every platform). Provenance tries exact spelling, then a unique SQLite `NOCASE` match. Case-only variants are not merged.
7. **Safety**: `exor-storage` and `exor-engine` forbid `unsafe`; all FFI is isolated in `exor-win` with a SAFETY note per block. No `unwrap`/`expect` outside tests (clippy-enforced).
8. **USN reads** are kernel-side waits on the blocking pool with a 1 s timeout; no sleep polling.
9. **Live registry visibility** is observe-only in the optional driver; live file visibility stays in user mode via the USN journal.
10. **GUI data** is per-user: `%LOCALAPPDATA%\ExorSystem` on Windows (SQLite and `quarantine`). A profile lock serializes desktop instances. An `asInvoker` manifest and token check remove the elevated desktop boundary; old adjacent `data` is not imported.

## Program inventory and review
The desktop shell merges classic uninstall entries, the current user's WinRT package list, and portable entries from SQLite. Only a live machine-wide classic registry entry can supply a supported uninstall command. Both the read-only command preview and the launch validate the live registration; the launch refuses a plan that differs from the reviewed executable and argument list. The desktop launches it without a shell or elevation; per-user or unsupported registrations are handled in Windows Settings. After a guided uninstall, the shell keeps a command-free copy in memory for leftover review; it is discarded on exit. The engine compares a bounded list of Software key names, SCM services, and task definitions. System findings are advisory and never flow to the filesystem quarantine command.

## Driverless install watch

The desktop uses before/after snapshots, not live USN or driver events. On Windows, the registry probe opens only selected Software subtrees with read access, using separate machine views. It records paths and key last-write stamps in memory, never value data. Each subtree is bounded by key count, depth and path storage; unreadable, linked, or over-limit trees are discarded for that sample. The engine compares complete pairs by path (ignoring ASCII case) and stamp. Only differences reach SQLite as `snapshot_diff` with heuristic attribution; removing a key does not create a cleanup candidate. The snapshots are not atomic or process-attributed.

## Change history

History queries the append-only ledger directly, including observations without a known node. Session, literal path, and optional registry/file/system domain filters apply before paging. The domain comes from the node type or, for unlinked observations, the recorded path/source. No schema migration is needed. An exclusive `entry_id` cursor keeps older pages stable when new evidence arrives; the existing session index supports scoped reads without a migration. The inspector displays old and new file fingerprints when available and registry presence/last-write change evidence without value data. Timeline exposes every domain. The relationship graph uses recorded edges in both directions without inferring new app dependencies. Cleanup size totals come from stored manifests, not a fresh filesystem traversal.

## Known limitations
See [audit](../AUDIT.md) and the [release checklist](../ROADMAP.md#before-publishing-a-windows-build).

## Window layout
Main window: grouped vertical tool navigation, workspace header, content view, and status bar. The [dashboard guide](DASHBOARD.md) maps the views to their data sources. A second borderless window (`log`, fixed 420x380 logical px, not resizable, hidden by default) is positioned by `gui/src-tauri/src/dock.rs` against the main window's right edge. Closed, it sits inside the main window's footprint and is hidden. Opening shows it behind the main window, then steps its x position outward (14 steps, eased); closing reverses and hides. Move, resize and minimize events on the main window keep it attached. Log lines live in a 1000-line ring buffer and are pushed to the window by a `log` event.
