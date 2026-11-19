# Command line (`exor`)

`exor` is a read-only command line for scripts, CI and end-to-end tests. It reads the same data the desktop app shows, and prints one JSON object per run with a fixed exit code.

It changes nothing on the system. It never moves, deletes or uninstalls anything, never writes to the registry, and opens the ExorSystem profile read-only (never created, migrated or locked). The only files it writes are a snapshot you name with `--out` and a fix script you name with `--script`, and it never overwrites either. To review and quarantine leftovers or run an uninstaller, use the desktop app.

Like the desktop app, it refuses to run with administrator rights (exit code 3).

## Build

Releases after v0.3.0 include `exor.exe` in the portable ZIP, next to `ExorSystem.exe`; the NSIS and MSI installers don't include it. v0.3.0 and earlier don't have it. To build it from the repository root with the [pinned toolchain](DEVELOPMENT.md):

```sh
cargo build --release -p exor-cli
target/release/exor help
```

## Commands

| Command | What it returns |
|---|---|
| `exor programs [--all]` | Programs Windows lists as installed (registry uninstall entries). `--all` includes system components and updates. Windows only. |
| `exor leftovers <program-id>` | Folders the program may have left behind, each with a reason, a confidence, and whether the desktop app may quarantine it. Use an `id` from `exor programs`. Suggestions only. |
| `exor sessions` | Recorded installation sessions from the profile. |
| `exor history [--session <id>] [--path <text>] [--limit <n>] [--before <id>]` | Recorded changes, newest first, up to `--limit` (1–1000, default 100). For the next page, pass the result's `next_before_id` as `--before`. |
| `exor snapshot <folder> [--out <file>] [--strict]` | Captures a folder and returns its file count, digest and how many paths could not be read. `--out` writes a snapshot file for `diff`. `--strict` hashes every file's content. |
| `exor diff <before-file> <after-file>` | Files added, removed and modified between two snapshot files. A path the later capture couldn't read is listed in `unreadable_in_after`, not as removed. |
| `exor env [--script <file.ps1>]` | PATH and toolchain-variable hygiene for the user and machine scopes (see below). Windows only. |
| `exor version` | Version, the profile schema it reads, and the snapshot file format. |
| `exor help` | Usage text (plain text, not JSON). |

Global options, accepted anywhere: `--pretty` indents the JSON, and `--profile <folder>` reads a different profile folder (for example a restored copy). The default is the desktop app's per-user profile, `%LOCALAPPDATA%\ExorSystem`.

## Output and exit codes

Success: `{"ok": true, "command": "<name>", "data": {...}}`
Failure: `{"ok": false, "command": "<name>", "error": {"kind": "<kind>", "message": "<plain text>"}}`

| Exit code | `kind` | Meaning |
|---|---|---|
| 0 | | Success |
| 1 | `failed` | The command could not finish (unreadable file, not a snapshot, I/O error) |
| 2 | `usage` | Unknown command or option, or a missing value |
| 3 | `refused` | Not allowed: run as administrator, an `--out` file already exists, or the profile schema doesn't match this build |
| 4 | `not_found` | No such program id, folder, snapshot file or profile |

The profile must be at the schema this build understands (`exor version` shows it). Open the desktop app once to upgrade an older profile.

## Environment check (`exor env`)

Reads `Path`, `GOPATH`, `NODE_PATH` and `PYTHONPATH`, plus toolchain variables such as `JAVA_HOME`, `CARGO_HOME`, `GOROOT`, `ANDROID_HOME` and `DOTNET_ROOT`. It reads them exactly as stored for your account (`HKCU\Environment`) and for the machine, keeping `%NAME%` references as written. Each finding has a `kind`, an `action` and a plain explanation.

| `kind` | `action` | Meaning |
|---|---|---|
| `duplicate` | `remove` | The same folder appears earlier in the variable (case, slashes and a trailing `\` don't matter) |
| `duplicate_of_machine` | `remove` | A user PATH entry the machine PATH already has; Windows searches the machine PATH first |
| `empty` | `remove` | An empty entry (`;;`); a single trailing `;` is ignored |
| `relative` | `review` | Not a full path, so it resolves against whatever folder a program runs in, a hijacking risk |
| `unresolved` | `review` | Still contains `%NAME%` after expansion: that variable isn't set |
| `missing` | `review` | The folder doesn't exist (an uninstalled program, or a drive that isn't connected) |
| `not_a_folder` | `review` | It names a file |

`--script fix.ps1` also writes a PowerShell script for you to read and run yourself; `exor` never runs it. The script:
- rebuilds each affected variable without the `remove` entries;
- keeps every `review` entry, marked `# REVIEW:` (delete a line to drop that entry);
- leaves toolchain variables alone unless you uncomment the line that clears one;
- keeps the stored type, so `%SystemRoot%`-style entries stay `REG_EXPAND_SZ`;
- embeds the original values: `-Undo` puts them back.

Machine variables are changed only from an elevated PowerShell. Programs that are already running keep their old environment until restarted.

## Snapshot files

`snapshot --out` writes JSON with `format: "exorsystem-snapshot"` and `version: 1`. It holds the root, the capture time, every entry with its size, modified time and BLAKE3 hash when computed, and the paths that couldn't be read, with reasons. `complete: false` means some paths weren't read. Treat what's missing as unknown, not as absent.

## Example: watch an install from a script

```sh
exor snapshot "C:\Program Files" --out before.json
# run the installer
exor snapshot "C:\Program Files" --out after.json
exor diff before.json after.json --pretty
```
