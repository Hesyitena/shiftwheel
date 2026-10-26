use crate::hook;
use crate::settings::{is_autostart_enabled, load_settings, save_settings, set_autostart};
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter};

pub fn create_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let settings = load_settings();
    let autostart = is_autostart_enabled();

    let enabled_item = CheckMenuItemBuilder::with_id("enabled", "Enabled")
        .checked(settings.enabled)
        .build(app)?;
    let sounds_item = CheckMenuItemBuilder::with_id("sounds", "Sounds")
        .checked(settings.sound)
        .build(app)?;
    let autostart_item = CheckMenuItemBuilder::with_id("autostart", "Start with Windows")
        .checked(autostart)
        .build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let info_item = MenuItemBuilder::with_id("info", concat!("Shiftwheel v", env!("CARGO_PKG_VERSION")))
        .enabled(false)
        .build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[
            &enabled_item,
            &sounds_item,
            &autostart_item,
            &sep,
            &info_item,
            &quit_item,
        ])
        .build()?;

    let enabled_clone = enabled_item.clone();
    let sounds_clone = sounds_item.clone();
    let autostart_clone = autostart_item.clone();

    let tray = TrayIconBuilder::new()
        .icon(tauri::image::Image::from_bytes(include_bytes!("../../assets/tray-32.png"))?)
        .tooltip("Shiftwheel")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app_handle, event| {
            hook::reset_drag();
            match event.id().as_ref() {
                "enabled" => {
                    // The menu has already toggled the check mark; read the new state.
                    let new_state = enabled_clone.is_checked().unwrap_or(true);
                    hook::set_enabled(new_state);
                    let mut s = load_settings();
                    s.enabled = new_state;
                    save_settings(&s);
                }
                "sounds" => {
                    let mut s = load_settings();
                    // The menu has already toggled the check mark; read the new state.
                    let new_state = sounds_clone.is_checked().unwrap_or(true);
                    s.sound = new_state;
                    save_settings(&s);
                    let _ = app_handle.emit("overlay:settings", serde_json::json!({ "sound": s.sound }));
                }
                "autostart" => {
                    // The menu has already toggled the check mark; read the new state.
                    let new_state = autostart_clone.is_checked().unwrap_or(false);
                    set_autostart(new_state);
                }
                "quit" => {
                    app_handle.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(tray)
}
