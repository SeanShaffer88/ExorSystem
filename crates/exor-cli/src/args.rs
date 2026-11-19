//! Command-line parsing: a small, dependency-free parser with strict errors.

use std::ffi::OsString;
use std::path::PathBuf;

/// One command and its options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print usage.
    Help,
    /// Version and profile schema.
    Version,
    /// Programs Windows lists as installed.
    Programs {
        /// Include system components and updates.
        all: bool,
    },
    /// Folders a program may have left behind. Read-only.
    Leftovers {
        /// Program id from `exor programs`.
        id: String,
    },
    /// Recorded installation sessions.
    Sessions,
    /// Recorded changes, newest first, one page at a time.
    History {
        /// Only this session.
        session: Option<i64>,
        /// Only paths containing this text (case-insensitive).
        path: Option<String>,
        /// Page size, 1–1000.
        limit: usize,
        /// Continue strictly before this entry id (`next_before_id` of the previous page).
        before: Option<i64>,
    },
    /// Capture a folder: summary, and optionally a snapshot file for `diff`.
    Snapshot {
        /// Folder to read.
        root: PathBuf,
        /// Write the snapshot here (never overwrites).
        out: Option<PathBuf>,
        /// Hash every file's content (slower, but catches same-size, same-time edits).
        strict: bool,
    },
    /// Compare two snapshot files.
    Diff {
        /// Earlier snapshot.
        before: PathBuf,
        /// Later snapshot.
        after: PathBuf,
    },
}

impl Command {
    /// Name used in the JSON envelope.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Version => "version",
            Self::Programs { .. } => "programs",
            Self::Leftovers { .. } => "leftovers",
            Self::Sessions => "sessions",
            Self::History { .. } => "history",
            Self::Snapshot { .. } => "snapshot",
            Self::Diff { .. } => "diff",
        }
    }

    /// Commands that read the system or the profile, and so follow the no-elevation rule.
    /// Elevation only exists on Windows, so the check is compiled there only.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn needs_normal_user(&self) -> bool {
        !matches!(self, Self::Help | Self::Version)
    }
}

/// A full invocation: the command plus global options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// What to do.
    pub command: Command,
    /// Indent the JSON output.
    pub pretty: bool,
    /// Profile folder instead of the desktop app's default.
    pub profile: Option<PathBuf>,
}

pub const USAGE: &str = "\
exor - ExorSystem command line (read-only)

Usage: exor <command> [options] [--pretty] [--profile <folder>]

Commands:
  programs [--all]                      Programs Windows lists as installed
  leftovers <program-id>                Folders a program may have left behind
  sessions                              Recorded installation sessions
  history [--session <id>] [--path <text>] [--limit <n>] [--before <id>]
                                        Recorded changes, newest first (default 100 per page)
  snapshot <folder> [--out <file>] [--strict]
                                        Capture a folder; --out writes a file for diff,
                                        --strict hashes every file's content
  diff <before-file> <after-file>       Compare two snapshot files
  version                               Version and profile schema
  help                                  This text

Every command prints one JSON object: {\"ok\": true, \"command\", \"data\"} or
{\"ok\": false, \"command\", \"error\": {\"kind\", \"message\"}}.

Exit codes: 0 success, 1 failed, 2 usage error, 3 refused (for example when run
as administrator), 4 not found.

Nothing here changes files, the registry or the ExorSystem profile, except the
file you name with --out.";

fn text(arg: OsString) -> Result<String, String> {
    arg.into_string()
        .map_err(|_| "arguments must be valid Unicode".to_string())
}

struct Args {
    rest: std::vec::IntoIter<String>,
}

impl Args {
    fn value(&mut self, flag: &str) -> Result<String, String> {
        match self.rest.next() {
            Some(v) if !v.starts_with("--") => Ok(v),
            _ => Err(format!("{flag} needs a value")),
        }
    }

    fn number<T: std::str::FromStr>(&mut self, flag: &str) -> Result<T, String> {
        let v = self.value(flag)?;
        v.parse()
            .map_err(|_| format!("{flag} needs a whole number, not '{v}'"))
    }

    fn positional(&mut self, what: &str) -> Result<String, String> {
        match self.rest.next() {
            Some(v) if !v.starts_with("--") => Ok(v),
            Some(v) => Err(format!("expected {what}, found option '{v}'")),
            None => Err(format!("missing {what}")),
        }
    }

    fn unknown(flag: &str, command: &str) -> String {
        format!("unknown option '{flag}' for '{command}'")
    }
}

/// Parse the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let all: Vec<String> = args.into_iter().map(text).collect::<Result<_, _>>()?;
    let mut pretty = false;
    let mut profile = None;
    let mut rest = Vec::new();
    let mut it = all.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pretty" => pretty = true,
            "--profile" => match it.next() {
                Some(v) if !v.starts_with("--") => profile = Some(PathBuf::from(v)),
                _ => return Err("--profile needs a folder".into()),
            },
            _ => rest.push(a),
        }
    }
    let mut args = Args {
        rest: rest.into_iter(),
    };
    let command = match args.rest.next().as_deref() {
        None | Some("help" | "--help" | "-h") => Command::Help,
        Some("version" | "--version" | "-V") => Command::Version,
        Some("programs") => {
            let mut all = false;
            for a in args.rest.by_ref() {
                match a.as_str() {
                    "--all" => all = true,
                    f => return Err(Args::unknown(f, "programs")),
                }
            }
            Command::Programs { all }
        }
        Some("leftovers") => Command::Leftovers {
            id: args.positional("a program id")?,
        },
        Some("sessions") => Command::Sessions,
        Some("history") => {
            let (mut session, mut path, mut limit, mut before) = (None, None, 100, None);
            while let Some(a) = args.rest.next() {
                match a.as_str() {
                    "--session" => session = Some(args.number("--session")?),
                    "--path" => path = Some(args.value("--path")?),
                    "--limit" => limit = args.number("--limit")?,
                    "--before" => before = Some(args.number("--before")?),
                    f => return Err(Args::unknown(f, "history")),
                }
            }
            if !(1..=1000).contains(&limit) {
                return Err("--limit must be between 1 and 1000".into());
            }
            Command::History {
                session,
                path,
                limit,
                before,
            }
        }
        Some("snapshot") => {
            let root = PathBuf::from(args.positional("a folder")?);
            let (mut out, mut strict) = (None, false);
            while let Some(a) = args.rest.next() {
                match a.as_str() {
                    "--out" => out = Some(PathBuf::from(args.value("--out")?)),
                    "--strict" => strict = true,
                    f => return Err(Args::unknown(f, "snapshot")),
                }
            }
            Command::Snapshot { root, out, strict }
        }
        Some("diff") => Command::Diff {
            before: PathBuf::from(args.positional("the earlier snapshot file")?),
            after: PathBuf::from(args.positional("the later snapshot file")?),
        },
        Some(other) => return Err(format!("unknown command '{other}'; see 'exor help'")),
    };
    if let Some(extra) = args.rest.next() {
        return Err(format!(
            "unexpected argument '{extra}' for '{}'",
            command.name()
        ));
    }
    Ok(Invocation {
        command,
        pretty,
        profile,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Invocation, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_commands_and_global_options_anywhere() {
        assert_eq!(p(&[]).unwrap().command, Command::Help);
        assert_eq!(p(&["--version"]).unwrap().command, Command::Version);
        let i = p(&["--pretty", "programs", "--all", "--profile", "D:\\p"]).unwrap();
        assert_eq!(i.command, Command::Programs { all: true });
        assert!(i.pretty);
        assert_eq!(i.profile, Some(PathBuf::from("D:\\p")));
        assert_eq!(
            p(&[
                "history",
                "--session",
                "3",
                "--path",
                "Foo",
                "--limit",
                "5",
                "--before",
                "90"
            ])
            .unwrap()
            .command,
            Command::History {
                session: Some(3),
                path: Some("Foo".into()),
                limit: 5,
                before: Some(90)
            }
        );
        assert_eq!(
            p(&["snapshot", "C:\\Apps", "--strict", "--out", "a.json"])
                .unwrap()
                .command,
            Command::Snapshot {
                root: "C:\\Apps".into(),
                out: Some("a.json".into()),
                strict: true
            }
        );
        assert_eq!(
            p(&["diff", "a.json", "b.json"]).unwrap().command,
            Command::Diff {
                before: "a.json".into(),
                after: "b.json".into()
            }
        );
        assert_eq!(
            p(&["leftovers", "hklm64:Foo"]).unwrap().command,
            Command::Leftovers {
                id: "hklm64:Foo".into()
            }
        );
    }

    #[test]
    fn rejects_bad_usage_with_a_reason() {
        for (args, want) in [
            (&["frobnicate"][..], "unknown command"),
            (&["programs", "--everything"], "unknown option"),
            (&["history", "--limit"], "needs a value"),
            (&["history", "--limit", "many"], "whole number"),
            (&["history", "--limit", "0"], "between 1 and 1000"),
            (&["snapshot"], "missing a folder"),
            (&["snapshot", "--out", "x"], "found option"),
            (&["diff", "a.json"], "missing the later"),
            (&["sessions", "extra"], "unexpected argument"),
            (&["programs", "--profile"], "--profile needs a folder"),
        ] {
            let e = p(args).unwrap_err();
            assert!(e.contains(want), "{args:?}: {e}");
        }
    }
}
