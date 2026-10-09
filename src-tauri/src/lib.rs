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
        .plugin(tauri_plugin_positioner::init())
        .setup(|app| {
            use tauri::Manager;

            let accounts_path = app
                .path()
                .app_data_dir()
                .map_err(|_| std::io::Error::other("Account storage is unavailable"))?
                .join("accounts.json");
            let activation_app = app.handle().clone();
            let notifications =
                std::sync::Arc::new(notifications::NativeNotificationService::macos(
                    app.handle().clone(),
                    std::sync::Arc::new(move |target| {
                        commands::activate_notification_message(&activation_app, target);
                    }),
                ));
            let state = commands::AppState::new(accounts_path, notifications)
                .map_err(|_| std::io::Error::other("OAuth services are unavailable"))?;
            tray::setup(app)?;
            state.refresh_tray(app.handle());
            app.manage(state);
            let lifecycle_app = app.handle().clone();
            platform::install_macos_sleep_wake_observer(std::sync::Arc::new(move |request| {
                commands::request_scheduler_lifecycle(&lifecycle_app, request);
            }))
            .map_err(|_| std::io::Error::other("System lifecycle is unavailable"))?;
            commands::start_polling(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_account,
            commands::check_mail_now,
            commands::get_app_snapshot,
            commands::open_account,
            commands::remove_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zerochi");
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_account_is_registered_in_the_tauri_handler() {
        let source = include_str!("lib.rs");
        let handler = source
            .split("tauri::generate_handler![")
            .nth(1)
            .and_then(|value| value.split("]").next())
            .expect("invoke handler declaration");

        assert!(handler.contains("commands::open_account"));
        assert!(handler.contains("commands::check_mail_now"));
    }

    #[test]
    fn macos_bundle_declares_menu_bar_agent_before_runtime_startup() {
        let info_plist = include_str!("../Info.plist");

        assert!(info_plist.contains("<key>LSUIElement</key>"));
        assert!(info_plist.contains("<true/>"));
        assert!(!info_plist.contains("<key>LSBackgroundOnly</key>"));
    }
}
