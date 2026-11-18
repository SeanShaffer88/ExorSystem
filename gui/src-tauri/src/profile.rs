//! Storage accounting and explicit, verified backup/retention commands.

use crate::{AppState, dock, jobs};
use exor_engine::{backup, execute};
use exor_storage::Store;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

#[derive(Default, Serialize)]
pub(crate) struct Usage {
    database_bytes: u64,
    wal_bytes: u64,
    manifests_bytes: u64,
    held_bytes: u64,
    migration_backup_bytes: u64,
    other_bytes: u64,
    incomplete: bool,
}

#[tauri::command]
pub(crate) async fn storage_usage(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Usage, String> {
    let job = state.jobs.work(&app, "Measuring profile storage", None)?;
    let profile = state.data_dir.clone();
    jobs::run(job, move || {
        let mut usage = Usage::default();
        let mut pending = vec![(profile.clone(), 0)];
        let mut count = 0;
        while let Some((dir, depth)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                usage.incomplete = true;
                continue;
            };
            for entry in entries {
                count += 1;
                if count > 100_000 {
                    usage.incomplete = true;
                    return Ok(usage);
                }
                let Ok(entry) = entry else {
                    usage.incomplete = true;
                    continue;
                };
                let p = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&p) else {
                    usage.incomplete = true;
                    continue;
                };
                if meta.is_symlink() {
                    usage.incomplete = true;
                    continue;
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        usage.incomplete = true;
                        continue;
                    }
                }
                if meta.is_dir() {
                    if depth >= 64 {
                        usage.incomplete = true;
                    } else {
                        pending.push((p, depth + 1));
                    }
                    continue;
                }
                if !meta.is_file() {
                    continue;
                }
                let file = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
                let target = if p.parent() == Some(profile.as_path()) {
                    match file {
                        "exor.db" => &mut usage.database_bytes,
                        "exor.db-wal" | "exor.db-shm" | "exor.db-journal" => &mut usage.wal_bytes,
                        name if name.starts_with("exor.pre-v") && name.ends_with(".bak") => {
                            &mut usage.migration_backup_bytes
                        }
                        _ => &mut usage.other_bytes,
                    }
                } else if p.starts_with(profile.join("quarantine")) && file == "manifest.json" {
                    &mut usage.manifests_bytes
                } else if p.starts_with(profile.join("quarantine"))
                    && p.parent()
                        .and_then(Path::file_name)
                        .is_some_and(|n| n == "blobs")
                {
                    &mut usage.held_bytes
                } else {
                    &mut usage.other_bytes
                };
                *target = target.saturating_add(meta.len());
            }
        }
        Ok(usage)
    })
    .await
}

fn folder(path: String) -> Result<PathBuf, String> {
    if path.len() > 4096 || path.contains('\0') {
        return Err("Invalid directory path.".into());
    }
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err("Enter the full path to a local directory.".into());
    }
    Ok(path)
}

#[tauri::command]
pub(crate) async fn backup_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    destination: String,
) -> Result<backup::VerifiedCopy, String> {
    let parent = folder(destination)?;
    let job = state.jobs.work(&app, "Verifying profile backup", None)?;
    let profile = state.data_dir.clone();
    let result = jobs::run(job, move || {
        let store = Store::open_writer(&profile.join("exor.db")).map_err(|e| e.to_string())?;
        backup::create(&store, &profile, &parent).map_err(|e| e.to_string())
    })
    .await?;
    dock::log(&app, "info", "Verified profile backup saved.");
    Ok(result)
}

#[tauri::command]
pub(crate) async fn restore_profile_copy(
    app: AppHandle,
    state: State<'_, AppState>,
    backup_path: String,
    destination: String,
) -> Result<backup::VerifiedCopy, String> {
    let source = folder(backup_path)?;
    let dest = folder(destination)?;
    let profile = state.data_dir.clone();
    let job = state
        .jobs
        .work(&app, "Verifying restored profile copy", None)?;
    jobs::run(job, move || {
        backup::restore_copy(&profile, &source, &dest).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(crate) async fn rotate_archived_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    quarantine_id: String,
    backup_path: String,
) -> Result<usize, String> {
    let archive = folder(backup_path)?;
    let root = state.quarantine_root.clone();
    let db = state.data_dir.join("exor.db");
    let job = state
        .jobs
        .work(&app, "Verifying archived batch before local rotation", None)?;
    let result = jobs::run(job, move || {
        let store = Store::open_writer(&db).map_err(|e| e.to_string())?;
        execute::rotate_by_id(&store, &root, &quarantine_id, &archive)
            .map(|m| m.items.len())
            .map_err(|e| e.to_string())
    })
    .await?;
    dock::log(
        &app,
        "info",
        "Rotated a held batch after verifying its backup. History was retained.",
    );
    Ok(result)
}
