# `exor` CLI alpha (unreleased)

`exor` is a read-only client for saved ExorSystem evidence and for the live system. It is **not** in the v0.3.0 packages; later portable ZIPs include `exor.exe` next to `ExorSystem.exe`, and it can be built from source. Use it for scripts that need an archive page, a recorded session's ledger, the installed-program list, leftover suggestions, folder snapshots or an environment check. It does not run a Watch or uninstaller, move files, write the registry, elevate, or initialize a profile. The only files it writes are a new snapshot (`--out`) or fix script (`--script`), never over an existing file.

## Build and try it

Use the [pinned Rust toolchain](DEVELOPMENT.md) from the repository root:

```powershell
cargo build --locked -p exor-cli
.\target\debug\exor.exe --help
.\target\debug\exor.exe archive --db '.\profile-copy\exor.db' --search 'Editor' --status completed --limit 20
.\target\debug\exor.exe archive --db '.\profile-copy\exor.db' --search '#123'
.\target\debug\exor.exe history --db '.\profile-copy\exor.db' --session 123 --domain file --kind modified --limit 100
```

On Linux, use `./target/debug/exor` against a compatible profile copy. Replace the example path and ID with real values. `--db` is required. The Windows desktop stores its database at `%LOCALAPPDATA%\ExorSystem\exor.db`; prefer pointing the CLI at `exor.db` in a verified profile backup or snapshot. If you read an active local database, do not copy just `exor.db` out from under SQLite; its WAL may contain committed records. The CLI opens SQLite in read-only mode and does not create or migrate the database. The current storage schema is v5; the CLI refuses other versions. SQLite may use its normal WAL read coordination; each call is a separate read, not a snapshot held across commands.

## Commands and output

- **`archive`** returns recorded sessions, most recent IDs first. `--search` accepts a literal app-name substring (A–Z case-insensitive) or an exact `#ID` (quote it in scripts). `--status` accepts `monitoring`, `completed` or `failed`. Older records may have no saved comparison. `comparison: null` does not mean zero changes. In a saved comparison `recorded_changes: null` means no scopes compared, while `0` means compared scopes recorded none.
- **`history`** requires `--session` with a positive ID and returns that session's recorded observations. `--path` is a literal substring, `--domain` accepts `file`, `registry` or `system`, and `--kind` accepts `created`, `modified`, `deleted`, `renamed`, `quarantined` or `restored`. Check the archive for saved coverage; an empty observation page does not establish coverage. Archive output includes saved app names; history includes recorded paths and can include hashes. Review both before sharing or sending to CI logs.

Successful query output is one JSON object on stdout: `schema_version: 1`, `kind` (`watch_archive` or `session_history`), `rows`, and nullable `next_before_id`. History also returns `session_id`, with each row containing the stored `entry_id`, `entry`, `observed_at`, and `domain`. Empty matches for a valid query are a success with `rows: []`. The max page size is 50 (default 20) for archive and 1,000 (default 100) for history. To get the next page, repeat the command with the same filters and `--before-id <next_before_id>`. Stop when it is `null`. This is an exclusive ID bound, not an offset.

| Exit code | Meaning |
|---|---|
| `0` | Query succeeded, even when no rows matched; `--help` and `--version` also succeed and print text. |
| `1` | Profile, query, or output failure (including a missing session, mismatched schema, or unreadable database). |
| `2` | Command usage error, such as a missing option, unknown flag, or invalid limit. |

## Live-system commands

These read the computer, not the profile, so they take no `--db`. They refuse to run with administrator rights (exit `1`), like the desktop. `programs`, `leftovers` and `env` need Windows. Each prints one JSON object with `schema_version: 1` and a `kind`, and lists its results in `rows`.

| Command | `kind` | Returns |
|---|---|---|
| `exor programs [--all]` | `installed_programs` | Programs Windows lists as installed. `--all` includes system components and updates. |
| `exor leftovers --program ID` | `leftover_suggestions` | Folders the program may have left behind, with reason, confidence and `can_quarantine`. Use an `id` from `programs`. Suggestions only; review and quarantine in the desktop. |
| `exor snapshot --root PATH [--out FILE] [--strict]` | `folder_snapshot` | File count, digest and unreadable paths for a folder. `--out` writes an `exorsystem-snapshot` v1 file; `--strict` hashes every file's content. `complete: false` means some paths weren't read: unknown, not absent. |
| `exor diff --before FILE --after FILE` | `snapshot_diff` | Added, removed and modified entries between two snapshot files. A path the later capture couldn't read is in `unreadable_in_after`, not counted as removed. |
| `exor env [--script FILE]` | `environment_check` | PATH and toolchain-variable findings (below). |

### Environment check

`env` reads `Path`, `GOPATH`, `NODE_PATH` and `PYTHONPATH`, plus toolchain variables such as `JAVA_HOME`, `CARGO_HOME`, `GOROOT`, `ANDROID_HOME` and `DOTNET_ROOT`. It reads them exactly as stored for your account (`HKCU\Environment`) and for the machine, keeping `%NAME%` references as written.

| Finding `kind` | `action` | Meaning |
|---|---|---|
| `duplicate` | `remove` | The same folder appears earlier in the variable (case, slashes and a trailing `\` are ignored) |
| `duplicate_of_machine` | `remove` | A user PATH entry the machine PATH already has; Windows searches the machine PATH first |
| `empty` | `remove` | `;;` (a single trailing `;` is ignored) |
| `relative` | `review` | Not a full path, so it resolves against the current folder, a hijacking risk |
| `unresolved` | `review` | `%NAME%` left after expansion |
| `missing` | `review` | The folder doesn't exist |
| `not_a_folder` | `review` | It names a file |

`--script fix.ps1` also writes a PowerShell script for you to read and run yourself; `exor` never runs it. The script:
- rebuilds each affected variable without the `remove` entries;
- keeps every `review` entry, marked `# REVIEW:` (delete a line to drop that entry);
- leaves toolchain variables alone unless you uncomment the line that clears one;
- keeps the stored registry type (`REG_EXPAND_SZ` stays expandable);
- changes machine variables only from an elevated PowerShell;
- embeds the original values, so `-Undo` restores them.

Errors are written to stderr. This alpha doesn't implement automated removal, quiet mode, or profile modification. Those operations require their own review and recovery contracts.
