#![forbid(unsafe_code)]

/// Starts the native host.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}
