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
        .setup(|app| {
            use tauri::Manager;

            let state = commands::AppState::new()
                .map_err(|_| std::io::Error::other("OAuth services are unavailable"))?;
            app.manage(state);
            tray::setup(app)
        })
        .invoke_handler(tauri::generate_handler![commands::add_account])
        .run(tauri::generate_context!())
        .expect("error while running Zerochi");
}
