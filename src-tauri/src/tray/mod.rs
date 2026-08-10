use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, WindowEvent,
};

const MAIN_WINDOW_LABEL: &str = "main";
const QUIT_MENU_ID: &str = "quit";

fn show_popup(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    let _ = window.show();
    let _ = window.set_focus();
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    app.handle()
        .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.hide()?;

        let popup = window.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Focused(false) => {
                let _ = popup.hide();
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = popup.hide();
            }
            _ => {}
        });
    }

    let menu = MenuBuilder::new(app)
        .text(QUIT_MENU_ID, "Cerrar Zerochi")
        .build()?;

    TrayIconBuilder::with_id("zerochi")
        .title("(•ᴗ•) 8")
        .tooltip("Zerochi — 8 pendientes (mock)")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == QUIT_MENU_ID {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_popup(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
