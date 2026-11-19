//! Read-only CLI alpha. No capture, privilege changes, process launch, or profile initialization.

use exor_storage::{
    ChangeDomain, ChangeKind, LedgerEntry, LedgerQuery, SessionStatus, Store, WatchArchivePage,
    WatchArchiveQuery, latest_version,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

const HELP: &str = "ExorSystem CLI alpha — read saved evidence (JSON)\n\
\n\
Usage:\n  exor archive --db PATH [--search TEXT|#ID] [--status STATUS] [--limit N] [--before-id ID]\n\
  exor history --db PATH --session ID [--path TEXT] [--domain DOMAIN] [--kind KIND]\n\
                                    [--limit N] [--before-id ID]\n\n\
Status: monitoring, completed, failed\n\
Domain: file, registry, system\n\
Kind: created, modified, deleted, renamed, quarantined, restored\n\
Archive limits: 1–50 (default 20). History limits: 1–1000 (default 100).\n\
Use next_before_id with the same command and filters to read an older page.\n\
An empty History page does not establish comparison coverage.\n";

#[derive(Serialize)]
struct Output {
    schema_version: u8,
    #[serde(flatten)]
    evidence: Evidence,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Evidence {
    WatchArchive {
        #[serde(flatten)]
        page: WatchArchivePage,
    },
    SessionHistory {
        session_id: i64,
        rows: Vec<LedgerEntry>,
        next_before_id: Option<i64>,
    },
}

struct Request {
    db: PathBuf,
    limit: usize,
    before_id: Option<i64>,
    query: Query,
}

enum Query {
    Archive {
        search: Option<String>,
        status: Option<SessionStatus>,
    },
    History {
        session_id: i64,
        path: Option<String>,
        domain: Option<ChangeDomain>,
        kind: Option<ChangeKind>,
    },
}

fn read_evidence(request: Request) -> Result<Output, String> {
    if !request.db.is_file() {
        return Err("--db must name an existing profile database file".into());
    }
    let store = Store::open_reader(&request.db).map_err(|e| e.to_string())?;
    let found = store.schema_version().map_err(|e| e.to_string())?;
    if found != latest_version() {
        return Err(format!(
            "profile schema {found} does not match expected schema {}; this CLI does not migrate profiles",
            latest_version()
        ));
    }
    let evidence = match request.query {
        Query::Archive { search, status } => Evidence::WatchArchive {
            page: store
                .watch_archive(
                    request.limit,
                    WatchArchiveQuery {
                        search: search.as_deref(),
                        status,
                        before_id: request.before_id,
                    },
                )
                .map_err(|e| e.to_string())?,
        },
        Query::History {
            session_id,
            path,
            domain,
            kind,
        } => {
            if store
                .session(session_id)
                .map_err(|e| e.to_string())?
                .is_none()
            {
                return Err(format!("no recorded session #{session_id}"));
            }
            let page = store
                .ledger_query(
                    request.limit,
                    LedgerQuery {
                        path_contains: path.as_deref(),
                        session_id: Some(session_id),
                        domain,
                        change_kind: kind,
                        before_id: request.before_id,
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())?;
            Evidence::SessionHistory {
                session_id,
                rows: page.entries,
                next_before_id: page.next_before_id,
            }
        }
    };
    Ok(Output {
        schema_version: 1,
        evidence,
    })
}

type Options = BTreeMap<&'static str, OsString>;

fn text(options: &mut Options, key: &str, max_bytes: usize) -> Result<Option<String>, String> {
    options
        .remove(key)
        .map(|arg| {
            let arg = arg
                .into_string()
                .map_err(|_| format!("{key} must be UTF-8 text"))?;
            if arg.is_empty() || arg.len() > max_bytes || arg.chars().any(char::is_control) {
                return Err(format!(
                    "{key} requires 1–{max_bytes} bytes without control characters"
                ));
            }
            Ok(arg)
        })
        .transpose()
}

fn option<T: FromStr>(options: &mut Options, key: &str) -> Result<Option<T>, String> {
    text(options, key, 40)?
        .map(|arg| arg.parse().map_err(|_| format!("invalid value for {key}")))
        .transpose()
}

fn positive_id(options: &mut Options, key: &str) -> Result<Option<i64>, String> {
    let id = option::<i64>(options, key)?;
    if id.is_some_and(|id| id <= 0) {
        return Err(format!("{key} must be a positive ID"));
    }
    Ok(id)
}

fn parse(command: &str, mut args: impl Iterator<Item = OsString>) -> Result<Request, String> {
    let (specific, default_limit, max_limit): (&[&str], _, _) = match command {
        "archive" => (&["--search", "--status"], 20, 50),
        "history" => (&["--session", "--path", "--domain", "--kind"], 100, 1000),
        _ => return Err("expected the archive or history command".into()),
    };
    let mut options = Options::new();
    while let Some(arg) = args.next() {
        let key = ["--db", "--limit", "--before-id"]
            .into_iter()
            .chain(specific.iter().copied())
            .find(|key| arg == *key)
            .ok_or_else(|| format!("unknown option for {command}"))?;
        let value = args
            .next()
            .filter(|value| !value.to_str().is_some_and(|value| value.starts_with("--")))
            .ok_or_else(|| format!("missing value after {key}"))?;
        if options.insert(key, value).is_some() {
            return Err(format!("{key} may be supplied only once"));
        }
    }
    let db = options
        .remove("--db")
        .filter(|path| !path.is_empty())
        .ok_or("--db PATH is required")?
        .into();
    let limit = option::<usize>(&mut options, "--limit")?.unwrap_or(default_limit);
    if !(1..=max_limit).contains(&limit) {
        return Err(format!(
            "{command} --limit must be between 1 and {max_limit}"
        ));
    }
    let before_id = positive_id(&mut options, "--before-id")?;
    let query = if command == "archive" {
        Query::Archive {
            search: text(&mut options, "--search", 240)?,
            status: option(&mut options, "--status")?,
        }
    } else {
        Query::History {
            session_id: positive_id(&mut options, "--session")?
                .ok_or("--session ID is required")?,
            path: text(&mut options, "--path", 4096)?,
            domain: option(&mut options, "--domain")?,
            kind: option(&mut options, "--kind")?,
        }
    };
    Ok(Request {
        db,
        limit,
        before_id,
        query,
    })
}

fn write_output(output: Output) -> Result<(), String> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut out, &output).map_err(|e| e.to_string())?;
    out.write_all(b"\n")
        .and_then(|()| out.flush())
        .map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1).peekable();
    let command = args.next();
    let name = command.as_deref().and_then(|name| name.to_str());
    let help = match name {
        Some("--help" | "-h") => args.peek().is_none(),
        Some("archive" | "history")
            if args
                .peek()
                .is_some_and(|arg| arg == "--help" || arg == "-h") =>
        {
            args.next();
            if args.peek().is_some() {
                eprintln!("exor: --help does not accept other options.");
                return ExitCode::from(2);
            }
            true
        }
        _ => false,
    };
    if help {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if matches!(name, Some("--version" | "-V")) && args.peek().is_none() {
        println!("exor {} (CLI alpha)", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let request = match parse(name.unwrap_or_default(), args) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("exor: {e}. See exor --help.");
            return ExitCode::from(2);
        }
    };
    match read_evidence(request).and_then(write_output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("exor: {e}");
            ExitCode::FAILURE
        }
    }
}
