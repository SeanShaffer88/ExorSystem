//! Standalone, read-only lab probe. Not used by the desktop or packaged in any installer.

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    match lab::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Live probe: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("The optional live-source probe only runs on Windows.");
    ExitCode::FAILURE
}

#[cfg(windows)]
mod lab {
    use exor_engine::events::{EventSource, RawEvent};
    use exor_win::{DriverRegistrySource, UsnJournalSource};
    use std::collections::BTreeMap;
    use std::time::Duration;

    pub fn run() -> Result<(), String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let usage =
            "usage: live-probe usn <drive-letter> <fixture-id> | live-probe registry <fixture-id>";
        let (mode, fixture) = match args.as_slice() {
            [mode, drive, id] if mode == "usn" => {
                let letter = drive
                    .chars()
                    .next()
                    .filter(|c| drive.len() == 1 && c.is_ascii_alphabetic())
                    .ok_or(usage)?;
                (Some(letter.to_ascii_uppercase()), id)
            }
            [mode, id] if mode == "registry" => (None, id),
            _ => return Err(usage.into()),
        };
        if fixture.len() != 32 || !fixture.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("use the ID from scripts/windows-fixture.ps1 -Action New".into());
        }
        let source: Box<dyn EventSource> = if let Some(volume) = mode {
            UsnJournalSource::probe(volume).map_err(|e| e.to_string())?;
            Box::new(UsnJournalSource::new(volume))
        } else {
            DriverRegistrySource::probe().map_err(|e| e.to_string())?;
            Box::new(DriverRegistrySource)
        };
        let fixture_key = format!("ExorSystem Roadmap {fixture}");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|e| e.to_string())?;
        let result = runtime.block_on(async {
            let mut rx = source.start();
            let mut counts = BTreeMap::<String, u64>::new();
            println!("Sampling for 30 seconds. Wait 2 seconds, then apply the fixture changes in another terminal.");
            let completed = tokio::time::timeout(Duration::from_secs(30), async {
                while let Some(event) = rx.recv().await {
                    let operation = match event {
                        RawEvent::Fs { path, reason, .. }
                            if path.split('\\').any(|part| part.eq_ignore_ascii_case(fixture)) =>
                        {
                            format!("{reason:?}")
                        }
                        RawEvent::Registry { key, op, .. }
                            if key.split('\\').any(|part| part.eq_ignore_ascii_case(&fixture_key)) =>
                        {
                            format!("{op:?}")
                        }
                        _ => continue,
                    };
                    *counts.entry(operation).or_default() += 1;
                }
            })
            .await;
            for (kind, count) in &counts {
                println!("{kind}: {count}");
            }
            if completed.is_ok() {
                Err("the source disconnected before the deadline; the sample is incomplete".into())
            } else if counts.is_empty() {
                Err("no fixture events matched; check the volume, driver connection, and fixture timing".into())
            } else {
                Ok(())
            }
        });
        // A port wait may remain blocked until another message or unloading. This short-lived
        // process exits instead of letting that wait hang an interactive desktop or its caller.
        runtime.shutdown_timeout(Duration::from_secs(1));
        result
    }
}
