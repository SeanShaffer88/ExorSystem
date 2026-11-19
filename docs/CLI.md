# `exor` CLI alpha (unreleased)

`exor` is a read-only client for saved ExorSystem evidence. It is built from source and is **not** in the v0.3.0 packages. Use it for scripts that need an archive page or a recorded session's ledger. It does not run a Watch or uninstaller, move files, access the registry, elevate, or initialize a profile.

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

Errors are written to stderr. This alpha doesn't implement automated removal, quiet mode, or profile modification. Those operations require their own review and recovery contracts.
