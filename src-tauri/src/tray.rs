use crate::hook;
use crate::settings::{is_autostart_enabled, load_settings, save_settings, set_autostart};
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter};

pub fn create_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let settings = load_settings();
    let autostart = is_autostart_enabled();

    let enabled_item = CheckMenuItemBuilder::with_id("enabled", "Enabled")
        .checked(true)
        .build(app)?;
    let sounds_item = CheckMenuItemBuilder::with_id("sounds", "Sounds")
        .checked(settings.sound)
        .build(app)?;
    let autostart_item = CheckMenuItemBuilder::with_id("autostart", "Start with Windows")
        .checked(autostart)
        .build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let info_item = MenuItemBuilder::with_id("info", "Shiftwheel v1.0.0")
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
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Shiftwheel")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app_handle, event| {
            hook::reset_drag();
            match event.id().as_ref() {
                "enabled" => {
                    let current = enabled_clone.is_checked().unwrap_or(true);
                    let new_state = !current;
                    let _ = enabled_clone.set_checked(new_state);
                    hook::set_enabled(new_state);
                }
                "sounds" => {
                    let mut s = load_settings();
                    let current = sounds_clone.is_checked().unwrap_or(true);
                    let new_state = !current;
                    let _ = sounds_clone.set_checked(new_state);
                    s.sound = new_state;
                    save_settings(&s);
                    let _ = app_handle.emit("overlay:settings", serde_json::json!({ "sound": s.sound }));
                }
                "autostart" => {
                    let current = autostart_clone.is_checked().unwrap_or(false);
                    let new_state = !current;
                    let _ = autostart_clone.set_checked(new_state);
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
