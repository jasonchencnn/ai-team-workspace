#![forbid(unsafe_code)]

/// Starts the native host with its private installation identity source.
/// No IPC bootstrap command or client-controlled Owner ID is exposed.
pub fn run() -> tauri::Result<()> {
    use tauri::Manager;
    tauri::Builder::default()
        .setup(|app| {
            let root = app.path().app_local_data_dir()?;
            let owner = security::installation_owner::open_installation(&root)?;
            app.manage(owner);
            Ok(())
        })
        .run(tauri::generate_context!())
}
