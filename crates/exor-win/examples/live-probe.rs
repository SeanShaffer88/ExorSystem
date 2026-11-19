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
    use exor_engine::Cancel;
    use exor_engine::events::{LiveEnd, LiveEventSource, RawEvent};
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
        let source: Box<dyn LiveEventSource> = if let Some(volume) = mode {
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
            let stop = Cancel::new();
            let (mut rx, task) = source.start_live(stop.clone());
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
            stop.cancel();
            drop(rx);
            let end = tokio::time::timeout(Duration::from_secs(5), task).await.map_err(|_| "Source did not stop within 5 seconds; collect lab diagnostics.")?.map_err(|e| e.to_string())?;
            if completed.is_ok() || end != LiveEnd::Stopped {
                Err(format!("Incomplete live sample: {end:?}"))
            } else if counts.is_empty() {
                Err("no fixture events matched; check the volume, driver connection, and fixture timing".into())
            } else {
                Ok(())
            }
        });
        // The lab timeout reports an incomplete sample. Never use this path in the desktop.
        runtime.shutdown_timeout(Duration::from_secs(1));
        result
    }
}
