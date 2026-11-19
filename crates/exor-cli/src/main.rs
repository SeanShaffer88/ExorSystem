//! `exor`: the ExorSystem command line. Read-only commands with one JSON object on stdout and a
//! stable exit code, for scripts, CI and end-to-end tests.

mod args;
mod commands;

use commands::CliError;
use serde_json::{Value, json};
use std::io::Write;
use std::process::ExitCode;

fn print(value: &Value, pretty: bool) {
    let text = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .unwrap_or_else(|e| {
        format!("{{\"ok\":false,\"error\":{{\"kind\":\"failed\",\"message\":\"{e}\"}}}}")
    });
    let mut out = std::io::stdout().lock();
    // A closed pipe (for example `exor programs | head`) is not an error worth reporting.
    let _ = writeln!(out, "{text}");
}

fn main() -> ExitCode {
    let inv = match args::parse(std::env::args_os().skip(1)) {
        Ok(inv) => inv,
        Err(message) => {
            let e = CliError::usage(message);
            print(
                &json!({ "ok": false, "command": null, "error": { "kind": e.kind, "message": e.message } }),
                false,
            );
            return ExitCode::from(e.exit_code());
        }
    };
    if inv.command == args::Command::Help {
        println!("{}", args::USAGE);
        return ExitCode::SUCCESS;
    }
    let name = inv.command.name();
    match commands::run(&inv) {
        Ok(data) => {
            print(
                &json!({ "ok": true, "command": name, "data": data }),
                inv.pretty,
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            print(
                &json!({ "ok": false, "command": name, "error": { "kind": e.kind, "message": e.message } }),
                inv.pretty,
            );
            ExitCode::from(e.exit_code())
        }
    }
}
