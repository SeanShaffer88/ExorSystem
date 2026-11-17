fn main() {
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("exorsystem.manifest"));
    if let Err(e) =
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
    {
        eprintln!("tauri build failed: {e:#}");
        std::process::exit(1);
    }
}
