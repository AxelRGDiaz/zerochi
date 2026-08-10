mod accounts;
mod auth;
mod commands;
mod gmail;
mod navigation;
mod notifications;
mod platform;
mod secure_store;
mod state;
mod sync;
mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(tray::setup)
        .run(tauri::generate_context!())
        .expect("error while running Zerochi");
}
