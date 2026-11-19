// Explicit app ACL: custom commands are denied unless the caller window has a grant.
const COMMANDS: &[&str] = &[
    "app_info",
    "apply_cleanup",
    "backup_profile",
    "begin_watch",
    "cancel_scan",
    "cancel_watch",
    "cleanup_folders",
    "cleanup_plan_page",
    "clear_log",
    "discard_cleanup_plan",
    "export_report",
    "find_leftovers",
    "finish_watch",
    "forget_portable_program",
    "get_log",
    "history",
    "link_cleanup_session",
    "list_portable_programs",
    "list_programs",
    "list_quarantines",
    "list_sessions",
    "list_templates",
    "log_is_open",
    "log_status",
    "make_portable",
    "pick_path",
    "prepare_cleanup",
    "preview_uninstaller",
    "provenance",
    "purge_quarantine",
    "quarantine_batch",
    "recent_watch_runs",
    "register_portable_program",
    "restore_profile_copy",
    "restore_quarantine",
    "rotate_archived_batch",
    "run_uninstaller",
    "save_changes",
    "scan",
    "session_items",
    "set_log_open",
    "simulate",
    "stop_cleanup_preflight",
    "stop_watch_capture",
    "storage_usage",
    "update_risk",
    "watch_archive",
    "watch_comparison",
];

fn main() {
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("exorsystem.manifest"));
    if let Err(e) = tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(windows)
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    ) {
        eprintln!("tauri build failed: {e:#}");
        std::process::exit(1);
    }
}
