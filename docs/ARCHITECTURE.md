# Architecture

## Released desktop

```text
React / TypeScript workspace (Tauri v2 `main` WebView)
  |  allowlisted commands; progress Channels and log events
  v
exorsystem-gui (`gui/src-tauri`): user profile, Capture/Scan gate, review state
  +-- exor-engine: snapshots, diff, provenance, policy, cleanup, backup, export
  +-- exor-storage: SQLite WAL, migrations, graph, append-only ledger
  +-- exor-win: read-only Windows metadata, native pickers and window helpers
  +-- exor-fs (via the engine): file identity and no-replace filesystem operations

Tauri `log` WebView: separate permissions for Activity log only
```

| Component | Responsibility |
|---|---|
| `gui/` | Dockable workspace, side-by-side change lists, exact cleanup review, and keyboard controls. The Vite demo and browser test IPC fixtures are not native evidence. |
| `gui/src-tauri/` | Validates incoming commands, owns state and approval plans, and bounds concurrent work. The `main` capability serves the workspace; `log` can access only its log commands, not cleanup, profile, or vendor-uninstall operations. |
| `crates/exor-engine/` | Safe Rust logic for file snapshots, BLAKE3 fingerprints, diffing, comparison coverage, program/session evidence, policy, quarantine, and archives. |
| `crates/exor-storage/` | Safe Rust storage using SQLite WAL with transactional schema changes and ledger writes. |
| `crates/exor-win/` and `crates/exor-fs/` | Platform adapters: registry/inventory/SCM reads, token and known-folder checks, native window placement, open-file identities and bounded filesystem publication primitives. |

Desktop Watch is **before/after snapshots**, not continuous capture. The NTFS USN reader and registry port client live in `exor-win`; the optional C callback driver lives in [`driver/`](../driver/README.md). They use separate lab probes and are not used by desktop Watch. [Lab status and distribution gates](LIVE-COLLECTION.md).

## Profile and provenance

- The Windows `asInvoker` desktop rejects an already-elevated token. It uses the Local AppData known folder's `ExorSystem` directory, with a single-instance lock, SQLite database, and held files under `quarantine`. The old executable-adjacent `data` directory is not imported.
- Migrations are forward-only. `0001` establishes the data model; `0002` adds the ledger and quarantine; `0003` adds portable inventory and lookup indexes; `0004` adds saved Watch comparison coverage. `0005` adds explicit application/session links, quarantine batch/file indexes and restore/purge intent, and indexes for History. Old rows do not gain invented coverage or session links. The SQLite pre-migration copy is not a full profile backup; see the [v0.3.0 upgrade instructions](releases/v0.3.0.md#before-you-upgrade).
- Edges read `child <relationship> parent`, for example `artifact created_by installer` or `app depends_on library`. The graph rejects cycles and keeps the highest recorded confidence when re-adding an edge: manifest `1.0`, temporal `0.7`, heuristic `0.3`. Confidence is evidence strength, not verified ownership or cleanup authority. Lookup tries exact path spelling and then a unique ASCII-case-insensitive match; it does not merge case-only records.

## Program inventory and review

The shell merges classic uninstall registrations, current-user MSIX packages, opted-in portable entries, and local session records. Only a supported live machine-wide classic registration can supply a vendor invocation. Preview and launch resolve the registration separately; launch rejects changes to the reviewed executable or arguments, and does not fall back to a shell or elevation. Use Windows Settings for per-user or unsupported uninstallers. A recently uninstalled app can be reviewed for leftovers until the session ends. Software key names, service executables, and task matches are read-only suggestions, not file movement authority.

## Driverless install watch

Watch reads only selected folders, bounded Software subtrees, and opted-in service or task metadata. An absent scope can be a valid snapshot input; a read/hash failure, incomplete task read, or unreadable, redirected, or over-limit registry scope is skipped instead of treated as empty. Registry capture records key paths and last-write stamps, not values. Differences from usable pairs are recorded as `snapshot_diff` with heuristic attribution. The snapshots are not atomic and do not attribute work to a process.

File comparisons use path tries and BLAKE3 hashes; ordinary capture can reuse hashes for unchanged metadata, while **Strict file comparison** hashes the content again. Comparison only uses usable pairs. The engine commits the coverage summary, ledger rows, graph changes, and final status together. A comparison with no usable pairs can save incomplete coverage without observations; stopping before commit or a failed write does not save a comparison. The baseline is in memory and cannot resume after restart.

## Change history and exports

History reads the append-only ledger, including observations without a known node. The session, literal path, optional source domain, change kind, and UTC filters apply before paging. An exclusive `entry_id` cursor keeps older pages stable as new observations arrive. Saved coverage is read independently; older sessions without it remain unknown. The inspector shows recorded fingerprints and key-change evidence, not file contents or registry values. Timeline shows all domains; the relationship graph follows recorded links, not an exhaustive Windows dependency scan. JSON export uses the same requested filters across all domains and a bounded continuation cursor.

## Cleanup and archives

Policy simulation is a dry run: when rules overlap, keep outranks review, which outranks quarantine. Applying cleanup is separate from a policy recommendation. Application/session eligibility and a read-only preflight produce the exact, expiring file list. Apply consumes the reviewed plan, rechecks scope and identity, and is not a generic file-list command. Registry, service, and task observations are never quarantine candidates.

Quarantine stages the manifest before moving files, commits the record and ledger atomically, and checks disk state when recovering interrupted work. Windows reviewed moves use an opened file and preserve the same-volume requirement instead of copying across volumes. Restore never overwrites. A conflict retains its blob for review, and startup never resumes an interrupted purge. SQLite's file index is for browsing; the manifest and current disk checks govern the operation. Recorded byte totals are logical file lengths, not measured free space.

A verified profile copy contains a SQLite snapshot, manifests, and held bytes. Restore publishes a **separate** copy, not a new active database. Archive rotation rechecks the exact held batch before locally deleting its bytes. History remains. See the [audit remediation](AUDIT-REMEDIATION.md) for limits and the [verification matrix](../AUDIT.md) for evidence boundaries.

## Window layout

The [panel catalog](../gui/src/docking/panels.ts) and layout reducer own dock placement, focus, and sizing. The `exor.workspace.layout.v1` preference stores validated layout choices, not file paths or capture state. Initialized tool frames stay mounted across panel moves or hides, so a Watch or Scan is not restarted by rearranging the workspace. Explicit context links update a destination that is already open. Compact mode exposes one dock at a time without overwriting saved sizes. Applications scrolls its detail card independently or switches to a detail page in a narrow dock.

On Windows, `log` is an owned top-level window attached below `main`. Placement uses physical monitor work bounds; the main window reclaims maximized work area on log close without changing the saved normal bounds. The log's geometry/open state is not persisted. The status bar reads only the latest message and clear watermark via `log_status`; the log window reads the 1,000-line ring via `get_log`. Sequence numbers and a clear watermark prevent old events from resurrecting cleared messages. This ring is not the evidence ledger and accepts no commands from displayed text. See [workspace controls](DASHBOARD.md#arrange-the-workspace) and the [optional client checks](WINDOWS-RELEASE.md#5-accessibility-and-optional-client-evidence).

## Known limitations

See the [audit](../AUDIT.md#baseline-limits-that-affect-release-decisions) for scope, permissions, backup, and validation limits. [Driver-lab gates](LIVE-COLLECTION.md#distribution-gates-open) are separate from [desktop publishing](WINDOWS-RELEASE.md#6-tag-and-publish-the-tested-candidate).
