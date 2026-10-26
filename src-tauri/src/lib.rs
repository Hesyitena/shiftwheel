mod actions;
mod config;
mod converter;
mod hook;
mod settings;
mod tray;
mod window_manager;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

static SOUND_ON: AtomicBool = AtomicBool::new(true);
static LAST_EXEC_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Default, Clone)]
struct DragContext {
    mode: String,
    category: String,
    paths: Vec<String>,
}

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

fn resolve_segment(
    mode: &str,
    cat: &str,
    count: usize,
    lx: f64,
    ly: f64,
    conf: &config::WheelConfig,
) -> Option<String> {
    const C: f64 = 180.0;
    const R_INNER: f64 = 80.0;
    const R_OUTER: f64 = 166.0;

    let dx = lx - C;
    let dy = ly - C;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < R_INNER || dist > R_OUTER {
        return None;
    }

    let list = match mode {
        "actions" => conf.actions.clone(),
        "tools" => conf.tools.get(cat).cloned().unwrap_or_default(),
        _ => conf.convert.get(cat).cloned().unwrap_or_else(|| {
            conf.convert.get("fallback").cloned().unwrap_or_default()
        }),
    };

    let items: Vec<&config::WheelItem> = list
        .iter()
        .filter(|item| !item.multi.unwrap_or(false) || count > 1)
        .collect();

    if items.is_empty() {
        return None;
    }

    let step = 360.0 / (items.len() as f64);
    let mut angle = dy.atan2(dx).to_degrees() + 90.0 + step / 2.0;
    while angle < 0.0 {
        angle += 360.0;
    }
    angle %= 360.0;
    let idx = (angle / step).floor() as usize % items.len();
    Some(items[idx].id.clone())
}

fn execute_action(app: AppHandle, id: String, paths: Vec<String>) {
    if id.is_empty() || paths.is_empty() {
        return;
    }

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

            window_manager::place_and_show_hud(&app);

            for (i, p) in paths.iter().enumerate() {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string());

                let _ = app.emit(
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
                        log::info!("Converted {} to {:?}", p, out);
                        last_out = Some(out);
                        ok_count += 1;
                    }
                    Err(err) => {
                        log::error!("Conversion failed: {}", err);
                    }
                }

                let percent = ((i + 1) as f64) / (total as f64);
                let _ = app.emit(
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
            let _ = app.emit("hud:hide", ());
            std::thread::sleep(Duration::from_millis(250));
            window_manager::hide_hud(&app);

            if ok_count > 0 {
                if SOUND_ON.load(Ordering::SeqCst) {
                    let _ = app.emit("overlay:sound", "done");
                }
                if let Some(out) = last_out {
                    actions::reveal_in_explorer(&out.to_string_lossy());
                }
            } else if SOUND_ON.load(Ordering::SeqCst) {
                let _ = app.emit("overlay:sound", "error");
            }
        }
        "tool" => {
            let total = paths.len();
            let mut last_out = None;
            let mut ok_count = 0;

            window_manager::place_and_show_hud(&app);

            for (i, p) in paths.iter().enumerate() {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string());

                let _ = app.emit(
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
                let _ = app.emit(
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
            let _ = app.emit("hud:hide", ());
            std::thread::sleep(Duration::from_millis(250));
            window_manager::hide_hud(&app);

            if ok_count > 0 {
                if SOUND_ON.load(Ordering::SeqCst) {
                    let _ = app.emit("overlay:sound", "done");
                }
                if let Some(out) = last_out {
                    actions::reveal_in_explorer(&out.to_string_lossy());
                }
            } else if SOUND_ON.load(Ordering::SeqCst) {
                let _ = app.emit("overlay:sound", "error");
            }
        }
        "action" => {
            let name = paths
                .first()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Selection".to_string());

            window_manager::place_and_show_hud(&app);

            let label = match value {
                "copy-path" => "Copy path",
                "copy" => "Copy to clipboard",
                "copy-to" => "Copy to folder",
                "move-to" => "Move to folder",
                "terminal" => "Open Terminal",
                "reveal" => "Reveal in Explorer",
                _ => "Action",
            };

            let _ = app.emit(
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

            let _ = app.emit(
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
            let _ = app.emit("hud:hide", ());
            std::thread::sleep(Duration::from_millis(250));
            window_manager::hide_hud(&app);
        }
        _ => {}
    }
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

    let now = now_ms();
    let last = LAST_EXEC_MS.swap(now, Ordering::SeqCst);
    if now.saturating_sub(last) < 400 {
        return;
    }

    let app_clone = app.clone();
    std::thread::spawn(move || {
        execute_action(app_clone, id, paths);
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

            let drag_ctx = Arc::new(Mutex::new(DragContext {
                mode: "convert".to_string(),
                category: "fallback".to_string(),
                paths: Vec::new(),
            }));
            let drag_ctx_hook = drag_ctx.clone();
            let drag_ctx_win = drag_ctx.clone();

            if let Some(overlay_win) = app.get_webview_window("overlay") {
                let app_h = app.handle().clone();
                let conf = config::get_wheel_config();
                let overlay_for_closure = overlay_win.clone();

                overlay_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::DragDrop(dde) = event {
                        match dde {
                            tauri::DragDropEvent::Enter { paths, position } => {
                                let mut cat = "fallback".to_string();
                                let count = paths.len();
                                if let Some(first) = paths.first() {
                                    cat = converter::category_of(first.to_str().unwrap_or("")).to_string();
                                }
                                let scale = overlay_for_closure.scale_factor().unwrap_or(1.0);
                                let lx = position.x as f64 / scale;
                                let ly = position.y as f64 / scale;

                                let paths_str: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                                if let Ok(mut ctx) = drag_ctx_win.lock() {
                                    ctx.paths = paths_str.clone();
                                    ctx.category = cat.clone();
                                }

                                let _ = app_h.emit(
                                    "overlay:drag_enter",
                                    serde_json::json!({
                                        "category": cat,
                                        "count": count,
                                        "paths": paths_str,
                                        "x": lx,
                                        "y": ly,
                                    }),
                                );
                            }
                            tauri::DragDropEvent::Over { position } => {
                                let scale = overlay_for_closure.scale_factor().unwrap_or(1.0);
                                let lx = position.x as f64 / scale;
                                let ly = position.y as f64 / scale;
                                let _ = app_h.emit(
                                    "overlay:drag_over",
                                    serde_json::json!({ "x": lx, "y": ly }),
                                );
                            }
                            tauri::DragDropEvent::Drop { paths, position } => {
                                let scale = overlay_for_closure.scale_factor().unwrap_or(1.0);
                                let lx = position.x as f64 / scale;
                                let ly = position.y as f64 / scale;

                                let (mode, cat, all_paths) = {
                                    let mut ctx = drag_ctx_win.lock().unwrap();
                                    if !paths.is_empty() {
                                        ctx.paths = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                                        if let Some(first) = paths.first() {
                                            ctx.category = converter::category_of(first.to_str().unwrap_or("")).to_string();
                                        }
                                    }
                                    (ctx.mode.clone(), ctx.category.clone(), ctx.paths.clone())
                                };

                                let selected_id = resolve_segment(&mode, &cat, all_paths.len(), lx, ly, &conf);
                                log::info!("Drop event: selected_id={:?}, pos=({:.1}, {:.1})", selected_id, lx, ly);

                                hook::reset_drag();
                                window_manager::hide_overlay(&app_h);

                                let _ = app_h.emit(
                                    "overlay:drag_drop",
                                    serde_json::json!({
                                        "id": selected_id,
                                        "paths": all_paths,
                                        "x": lx,
                                        "y": ly,
                                    }),
                                );

                                if let Some(id) = selected_id {
                                    let now = now_ms();
                                    let last = LAST_EXEC_MS.swap(now, Ordering::SeqCst);
                                    if now.saturating_sub(last) >= 400 {
                                        if SOUND_ON.load(Ordering::SeqCst) {
                                            let _ = app_h.emit("overlay:sound", "select");
                                        }
                                        let h = app_h.clone();
                                        std::thread::spawn(move || {
                                            execute_action(h, id, all_paths);
                                        });
                                    }
                                } else {
                                    let dx = lx - 180.0;
                                    let dy = ly - 180.0;
                                    let dist = (dx * dx + dy * dy).sqrt();
                                    if dist < 80.0 && SOUND_ON.load(Ordering::SeqCst) {
                                        let _ = app_h.emit("overlay:sound", "cancel");
                                    } else if SOUND_ON.load(Ordering::SeqCst) {
                                        let _ = app_h.emit("overlay:sound", "close");
                                    }
                                }
                            }
                            tauri::DragDropEvent::Leave => {
                                let _ = app_h.emit("overlay:drag_leave", ());
                            }
                            _ => {}
                        }
                    }
                });
            }

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
                        if let Ok(mut ctx) = drag_ctx_hook.lock() {
                            ctx.mode = mode.clone();
                        }
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
