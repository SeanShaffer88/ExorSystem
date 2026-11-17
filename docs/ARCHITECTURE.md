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
4. **Policy precedence**: when several rules match, the most protective wins (keep, review, quarantine). No match means review.
5. **Case-insensitive paths** in the trie (Windows semantics on every platform).
6. **Safety**: `exor-storage` and `exor-engine` forbid `unsafe`; all FFI is isolated in `exor-win` with a SAFETY note per block. No `unwrap`/`expect` outside tests (clippy-enforced).
7. **USN reads** are kernel-side waits on the blocking pool with a 1 s timeout; no sleep polling.
8. **Registry visibility** is observe-only in the driver; file visibility stays in user mode via the USN journal.
9. **GUI data** is portable: `data/exor.db` beside the executable.

## Known limitations
See `AUDIT.md`.
