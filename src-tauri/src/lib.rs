mod actions;
mod config;
mod converter;
mod hook;
mod settings;
mod tray;
mod window_manager;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

static SOUND_ON: AtomicBool = AtomicBool::new(true);

#[tauri::command]
fn overlay_revealed(app: AppHandle, ok: bool) {
    if !ok {
        hook::reset_drag();
        window_manager::hide_overlay(&app);
    }
}

#[tauri::command]
fn crop_done(_rect: serde_json::Value) {
    // Interactive crop done handler
}

#[tauri::command]
fn overlay_drop(app: AppHandle, msg: serde_json::Value) {
    hook::reset_drag();
    window_manager::hide_overlay(&app);

    let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let paths: Vec<String> = msg
        .get("paths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    if id.is_empty() || paths.is_empty() {
        return;
    }

    let app_clone = app.clone();
    std::thread::spawn(move || {
        let parts: Vec<&str> = id.splitn(2, ':').collect();
        if parts.len() != 2 {
            return;
        }
        let kind = parts[0];
        let value = parts[1];

        match kind {
            "convert" => {
                let total = paths.len();
                let mut last_out = None;
                let mut ok_count = 0;

                window_manager::place_and_show_hud(&app_clone);

                for (i, p) in paths.iter().enumerate() {
                    let name = std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "file".to_string());

                    let _ = app_clone.emit(
                        "hud:update",
                        serde_json::json!({
                            "name": name,
                            "label": format!("Convert to {}", value.to_uppercase()),
                            "chip": value.to_uppercase(),
                            "index": i + 1,
                            "total": total,
                            "percent": null,
                        }),
                    );

                    match converter::convert_file(p, value) {
                        Ok(out) => {
                            last_out = Some(out);
                            ok_count += 1;
                        }
                        Err(err) => {
                            log::error!("Conversion failed: {}", err);
                        }
                    }

                    let percent = ((i + 1) as f64) / (total as f64);
                    let _ = app_clone.emit(
                        "hud:update",
                        serde_json::json!({
                            "name": name,
                            "label": if i + 1 == total { "Done!".to_string() } else { format!("Convert to {}", value.to_uppercase()) },
                            "chip": value.to_uppercase(),
                            "index": i + 1,
                            "total": total,
                            "percent": percent,
                        }),
                    );
                }

                std::thread::sleep(Duration::from_millis(1200));
                let _ = app_clone.emit("hud:hide", ());
                std::thread::sleep(Duration::from_millis(250));
                window_manager::hide_hud(&app_clone);

                if ok_count > 0 {
                    if SOUND_ON.load(Ordering::SeqCst) {
                        let _ = app_clone.emit("overlay:sound", "done");
                    }
                    if let Some(out) = last_out {
                        actions::reveal_in_explorer(&out.to_string_lossy());
                    }
                } else if SOUND_ON.load(Ordering::SeqCst) {
                    let _ = app_clone.emit("overlay:sound", "error");
                }
            }
            "tool" => {
                let total = paths.len();
                let mut last_out = None;
                let mut ok_count = 0;

                window_manager::place_and_show_hud(&app_clone);

                for (i, p) in paths.iter().enumerate() {
                    let name = std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "file".to_string());

                    let _ = app_clone.emit(
                        "hud:update",
                        serde_json::json!({
                            "name": name,
                            "label": format!("Tool: {value}"),
                            "chip": "TOOL",
                            "index": i + 1,
                            "total": total,
                            "percent": null,
                        }),
                    );

                    match converter::run_tool(value, &[p.clone()]) {
                        Ok(out) => {
                            last_out = Some(out);
                            ok_count += 1;
                        }
                        Err(err) => {
                            log::error!("Tool failed: {}", err);
                        }
                    }

                    let percent = ((i + 1) as f64) / (total as f64);
                    let _ = app_clone.emit(
                        "hud:update",
                        serde_json::json!({
                            "name": name,
                            "label": if i + 1 == total { "Done!".to_string() } else { format!("Tool: {value}") },
                            "chip": "TOOL",
                            "index": i + 1,
                            "total": total,
                            "percent": percent,
                        }),
                    );
                }

                std::thread::sleep(Duration::from_millis(1200));
                let _ = app_clone.emit("hud:hide", ());
                std::thread::sleep(Duration::from_millis(250));
                window_manager::hide_hud(&app_clone);

                if ok_count > 0 {
                    if SOUND_ON.load(Ordering::SeqCst) {
                        let _ = app_clone.emit("overlay:sound", "done");
                    }
                    if let Some(out) = last_out {
                        actions::reveal_in_explorer(&out.to_string_lossy());
                    }
                } else if SOUND_ON.load(Ordering::SeqCst) {
                    let _ = app_clone.emit("overlay:sound", "error");
                }
            }
            "action" => {
                let name = paths
                    .first()
                    .and_then(|p| std::path::Path::new(p).file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Selection".to_string());

                window_manager::place_and_show_hud(&app_clone);

                let label = match value {
                    "copy-path" => "Copy path",
                    "copy" => "Copy to clipboard",
                    "copy-to" => "Copy to folder",
                    "move-to" => "Move to folder",
                    "terminal" => "Open Terminal",
                    "reveal" => "Reveal in Explorer",
                    _ => "Action",
                };

                let _ = app_clone.emit(
                    "hud:update",
                    serde_json::json!({
                        "name": name,
                        "label": format!("{label}..."),
                        "chip": "ACTION",
                        "index": 1,
                        "total": 1,
                        "percent": null,
                    }),
                );

                match value {
                    "copy-path" => {
                        let _ = actions::copy_text_to_clipboard(&paths.join("\r\n"));
                    }
                    "copy" => {
                        let _ = actions::copy_files_to_clipboard(&paths);
                    }
                    "copy-to" => {
                        let _ = actions::transfer_files(&paths, false);
                    }
                    "move-to" => {
                        let _ = actions::transfer_files(&paths, true);
                    }
                    "terminal" => {
                        actions::open_terminal(&paths);
                    }
                    "reveal" => {
                        if let Some(first) = paths.first() {
                            actions::reveal_in_explorer(first);
                        }
                    }
                    _ => {}
                }

                let _ = app_clone.emit(
                    "hud:update",
                    serde_json::json!({
                        "name": name,
                        "label": format!("{label} · Done!"),
                        "chip": "ACTION",
                        "index": 1,
                        "total": 1,
                        "percent": 1.0,
                    }),
                );

                std::thread::sleep(Duration::from_millis(1200));
                let _ = app_clone.emit("hud:hide", ());
                std::thread::sleep(Duration::from_millis(250));
                window_manager::hide_hud(&app_clone);
            }
            _ => {}
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let s = settings::load_settings();
            SOUND_ON.store(s.sound, Ordering::SeqCst);

            let _tray = tray::create_tray(&app.handle())?;

            let app_handle = app.handle().clone();

            // Send config to overlay once ready
            let conf = config::get_wheel_config();
            let app_clone_init = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(600));
                let _ = app_clone_init.emit("overlay:config", conf);
                let _ = app_clone_init.emit("overlay:settings", serde_json::json!({ "sound": s.sound }));
            });

            // Start native low-level Windows hook
            hook::start_hook(move |event| {
                let handle = app_handle.clone();
                match event {
                    hook::HookEvent::OpenWheel { mode, x, y } => {
                        window_manager::place_and_show_overlay(&handle, x, y);
                        hook::set_shown(true);
                        let _ = handle.emit(
                            "overlay:show",
                            serde_json::json!({ "mode": mode, "fresh": true }),
                        );
                    }
                    hook::HookEvent::ScheduleHide => {
                        hook::set_shown(false);
                        let _ = handle.emit("overlay:close", ());
                        let h = handle.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(350));
                            window_manager::hide_overlay(&h);
                        });
                    }
                    hook::HookEvent::HideNow => {
                        hook::set_shown(false);
                        let _ = handle.emit("overlay:close", ());
                        window_manager::hide_overlay(&handle);
                    }
                    hook::HookEvent::ProbeCheck => {}
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            overlay_revealed,
            overlay_drop,
            crop_done,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
