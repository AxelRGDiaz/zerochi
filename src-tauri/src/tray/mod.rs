use tauri::{tray::TrayIconBuilder, App};

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    TrayIconBuilder::with_id("zerochi")
        .title("(•ᴗ•) 8")
        .tooltip("Zerochi — 8 pendientes (mock)")
        .build(app)?;
    Ok(())
}
