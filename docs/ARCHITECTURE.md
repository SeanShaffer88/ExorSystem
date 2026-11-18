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
2. **Schema**: `0001` is the blueprint schema verbatim. The two PRAGMAs run per connection (they cannot run inside a migration transaction). `0002` adds the change ledger, quarantine records and a unique edge index, which the blueprint's pillars require but its schema lacks.
3. **Confidence ladder**: manifest 1.0, temporal 0.7, heuristic 0.3. Re-adding an edge keeps the highest score, so evidence only ever raises confidence.
4. **Quarantine safety**: `quarantine` writes the manifest as `in_progress` before moving anything. On startup, `recover_interrupted` puts any blob still held back at its original path (never over a file that reappeared) and marks the manifest `rolled_back`.
5. **Policy precedence**: when several rules match, the most protective wins (keep, review, quarantine). No match means review.
6. **Case-insensitive paths** in the trie (Windows semantics on every platform).
7. **Safety**: `exor-storage` and `exor-engine` forbid `unsafe`; all FFI is isolated in `exor-win` with a SAFETY note per block. No `unwrap`/`expect` outside tests (clippy-enforced).
8. **USN reads** are kernel-side waits on the blocking pool with a 1 s timeout; no sleep polling.
9. **Registry visibility** is observe-only in the driver; file visibility stays in user mode via the USN journal.
10. **GUI data** keeps `data/exor.db` beside the executable. On Windows, quarantine manifests and blobs live in the system ProgramData folder under a per-installation directory; restore and purge resolve batches by ID inside that protected root, never from the portable database's stored path.

## Known limitations
See `AUDIT.md`.

## Window layout
Main window: tab bar, view, status bar. A second borderless window (`log`, fixed 420x380 logical px, not resizable, hidden by default) is positioned by `gui/src-tauri/src/dock.rs` against the main window's right edge. Closed, it sits inside the main window's footprint and is hidden. Opening shows it behind the main window, then steps its x position outward (14 steps, eased); closing reverses and hides. Move, resize and minimize events on the main window keep it attached. Log lines live in a 1000-line ring buffer and are pushed to the window by a `log` event.
