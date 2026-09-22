#![forbid(unsafe_code)]

/// Starts the native desktop host. Domain services are intentionally not
/// registered here until their bounded contexts are available.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}
