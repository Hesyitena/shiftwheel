mod actions;
mod config;
mod converter;
mod external;
mod fodt;
mod hook;
mod imaging;
mod jobs;
mod pdfops;
mod settings;
mod tray;
mod wheel;
mod window_manager;

use converter::CropRect;
use jobs::{CropRequest, HudMsg, Job, JobQueue, Ui};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// The drag the wheel is currently showing.
#[derive(Default)]
struct DragState {
    mode: String,
    paths: Vec<String>,
    /// True between a file drag entering the wheel and it leaving / dropping. Drops without an
    /// active file drag are ignored (WebView2 can report a drop for a later non-file drag).
    active: bool,
}

struct AppState {
    drag: Mutex<DragState>,
    config: config::WheelConfig,
    jobs: Mutex<Option<JobQueue>>,
    /// Bumped whenever the overlay is opened or hidden, so a delayed hide never hides a newer wheel.
    overlay_gen: AtomicU64,
    /// Same for the progress card.
    hud_gen: AtomicU64,
    last_exec_ms: AtomicU64,
    hud_reveal: Mutex<Option<PathBuf>>,
    crop_reply: Mutex<Option<Sender<Option<CropRect>>>>,
    crop_request: Mutex<Option<CropRequest>>,
    overlay_pos: Mutex<(i32, i32)>,
    last_cursor: Mutex<(f64, f64)>,
    /// The first-run card is waiting to be acknowledged.
    first_run_pending: AtomicBool,
}

fn state<R: Runtime>(app: &AppHandle<R>) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

// ---------------------------------------------------------------------------------------------
// Job UI backed by the real windows
// ---------------------------------------------------------------------------------------------

struct TauriUi {
    app: AppHandle,
}

impl Ui for TauriUi {
    fn hud(&self, msg: &HudMsg) {
        let st = state(&self.app);
        st.hud_gen.fetch_add(1, Ordering::SeqCst);
        *st.hud_reveal.lock().unwrap_or_else(|e| e.into_inner()) = None;
        window_manager::place_and_show_hud(&self.app, window_manager::HUD_SIZE);
        let _ = self.app.emit("hud:update", msg);
    }

    fn finish(&self, msg: &HudMsg, reveal: Option<PathBuf>) {
        let st = state(&self.app);
        let generation = st.hud_gen.fetch_add(1, Ordering::SeqCst) + 1;
        *st.hud_reveal.lock().unwrap_or_else(|e| e.into_inner()) = reveal;
        window_manager::place_and_show_hud(&self.app, window_manager::HUD_SIZE);
        let _ = self.app.emit("hud:update", msg);
        let app = self.app.clone();
        let linger = msg.linger_ms.max(1000);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(linger));
            let st = state(&app);
            if st.hud_gen.load(Ordering::SeqCst) != generation {
                return;
            }
            let _ = app.emit("hud:hide", ());
            std::thread::sleep(Duration::from_millis(250));
            if st.hud_gen.load(Ordering::SeqCst) == generation {
                window_manager::hide_hud(&app);
            }
        });
    }

    fn sound(&self, name: &str) {
        let _ = self.app.emit("overlay:sound", name);
    }

    /// The progress card is top-most; owning the dialog keeps it in front of Explorer.
    fn pick_folder(&self) -> Option<PathBuf> {
        #[cfg(windows)]
        let owner = self.app.get_webview_window("hud").and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
        #[cfg(not(windows))]
        let owner = None;
        actions::pick_folder(owner)
    }

    fn ask_crop(&self, req: CropRequest) -> Option<CropRect> {
        let st = state(&self.app);
        let (tx, rx) = channel();
        if let Some(old) = st.crop_reply.lock().unwrap_or_else(|e| e.into_inner()).replace(tx) {
            let _ = old.send(None);
        }
        *st.crop_request.lock().unwrap_or_else(|e| e.into_inner()) = Some(req.clone());
        window_manager::hide_hud(&self.app);
        if let Some(win) = self.app.get_webview_window("crop") {
            let _ = win.emit("crop:init", &req);
            let _ = win.center();
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        } else {
            return None;
        }
        let answer = rx.recv().unwrap_or(None);
        *st.crop_request.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(win) = self.app.get_webview_window("crop") {
            let _ = win.hide();
        }
        answer
    }
}

fn answer_crop<R: Runtime>(app: &AppHandle<R>, rect: Option<CropRect>) {
    let st = state(app);
    if let Some(tx) = st.crop_reply.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(rect);
    }
    if let Some(win) = app.get_webview_window("crop") {
        let _ = win.hide();
    }
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

/// The overlay asks for its settings once it has loaded (an event sent at startup could
/// arrive before the page is ready).
#[tauri::command]
fn get_settings() -> serde_json::Value {
    serde_json::json!({ "sound": settings::load_settings().sound })
}

#[tauri::command]
fn overlay_revealed<R: Runtime>(app: AppHandle<R>, ok: bool) {
    if !ok {
        hook::reset_drag();
        hook::set_shown(false);
        window_manager::hide_overlay(&app);
    }
}

#[tauri::command]
fn crop_done<R: Runtime>(app: AppHandle<R>, rect: Option<CropRect>) {
    answer_crop(&app, rect.filter(|r| r.w > 0.0 && r.h > 0.0));
}

#[tauri::command]
fn get_crop_request<R: Runtime>(app: AppHandle<R>) -> Option<CropRequest> {
    state(&app).crop_request.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn first_run_payload() -> serde_json::Value {
    serde_json::json!({ "autostart": settings::is_autostart_enabled() })
}

/// The first-run card asks for its data once loaded (the event may have come earlier).
#[tauri::command]
fn get_first_run<R: Runtime>(app: AppHandle<R>) -> Option<serde_json::Value> {
    state(&app).first_run_pending.load(Ordering::SeqCst).then(first_run_payload)
}

/// "Got it" on the first-run card.
#[tauri::command]
fn first_run_done<R: Runtime>(app: AppHandle<R>, autostart: bool) {
    let st = state(&app);
    st.first_run_pending.store(false, Ordering::SeqCst);
    let mut s = settings::load_settings();
    s.onboarded = true;
    settings::save_settings(&s);
    if settings::is_autostart_enabled() != autostart {
        settings::set_autostart(autostart);
    }
    if let Some(item) = app.try_state::<tray::AutostartItem<R>>() {
        let _ = item.0.set_checked(autostart);
    }
    st.hud_gen.fetch_add(1, Ordering::SeqCst);
    let _ = app.emit("hud:hide", ());
    window_manager::hide_hud(&app);
}

/// Click on the progress card: show the result in Explorer.
#[tauri::command]
fn hud_reveal<R: Runtime>(app: AppHandle<R>) {
    let st = state(&app);
    let path = st.hud_reveal.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(p) = path {
        if let Err(e) = actions::reveal_in_explorer(&p.to_string_lossy()) {
            log::error!("reveal failed: {e}");
        }
    }
    st.hud_gen.fetch_add(1, Ordering::SeqCst);
    let _ = app.emit("hud:hide", ());
    window_manager::hide_hud(&app);
}

// ---------------------------------------------------------------------------------------------
// Overlay events
// ---------------------------------------------------------------------------------------------

fn hide_overlay_now<R: Runtime>(app: &AppHandle<R>) {
    state(app).overlay_gen.fetch_add(1, Ordering::SeqCst);
    hook::reset_drag();
    hook::set_shown(false);
    window_manager::hide_overlay(app);
}

fn make_thumbnail(path: &str) -> Option<String> {
    let p = Path::new(path);
    if !p.exists() || !p.is_file() {
        return None;
    }
    if converter::category_of(path) != "image" {
        return None;
    }
    let img = imaging::load_image(p).ok()?;
    imgcore::preview_data_url(&img, 96).ok()
}

fn on_drag_drop(app: &AppHandle, scale: f64, event: &tauri::DragDropEvent) {
    let st = state(app);
    let to_logical = |p: &tauri::PhysicalPosition<f64>| (p.x / scale, p.y / scale);
    let to_strings = |paths: &[PathBuf]| -> Vec<String> { paths.iter().map(|p| p.to_string_lossy().to_string()).collect() };

    match event {
        tauri::DragDropEvent::Enter { paths, position } => {
            let (lx, ly) = to_logical(position);
            let paths = to_strings(paths);
            let first_path = paths.first().cloned();
            let file_name = first_path.as_ref().and_then(|p| {
                Path::new(p).file_name().map(|n| n.to_string_lossy().to_string())
            });
            let thumb = first_path.as_ref().and_then(|p| make_thumbnail(p));
            let items = {
                let mut d = st.drag.lock().unwrap_or_else(|e| e.into_inner());
                d.active = true;
                if !paths.is_empty() {
                    d.paths = paths.clone();
                }
                wheel::items_for(&st.config, &d.mode, &d.paths)
            };
            let _ = app.emit(
                "overlay:drag_enter",
                serde_json::json!({
                    "category": wheel::category_for(&paths),
                    "count": if paths.is_empty() { 1 } else { paths.len() },
                    "items": items,
                    "x": lx,
                    "y": ly,
                    "paths": paths,
                    "file_name": file_name,
                    "thumb": thumb,
                }),
            );
        }
        tauri::DragDropEvent::Over { position } => {
            let (lx, ly) = to_logical(position);
            *st.last_cursor.lock().unwrap_or_else(|e| e.into_inner()) = (lx, ly);
            let _ = app.emit("overlay:drag_over", serde_json::json!({ "x": lx, "y": ly }));
        }
        tauri::DragDropEvent::Leave => {
            st.drag.lock().unwrap_or_else(|e| e.into_inner()).active = false;
            let _ = app.emit("overlay:drag_leave", ());
        }
        tauri::DragDropEvent::Drop { paths, position } => {
            let (mut lx, mut ly) = to_logical(position);
            if lx == 0.0 && ly == 0.0 {
                let last = *st.last_cursor.lock().unwrap_or_else(|e| e.into_inner());
                lx = last.0;
                ly = last.1;
            }
            let paths = to_strings(paths);
            let mode = {
                let mut d = st.drag.lock().unwrap_or_else(|e| e.into_inner());
                d.active = false;
                d.paths.clear();
                d.mode.clone()
            };
            hide_overlay_now(app);

            if paths.is_empty() {
                let _ = app.emit("overlay:drag_drop", serde_json::json!({ "id": null }));
                return;
            }
            let items = wheel::items_for(&st.config, &mode, &paths);
            let id = wheel::resolve(&items, lx, ly);
            log::info!("drop at ({lx:.1}, {ly:.1}) -> {id:?}");
            let sound = match (&id, wheel::zone(lx, ly)) {
                (Some(_), _) => "select",
                (None, wheel::Zone::Hole) => "cancel",
                _ => "close",
            };
            let _ = app.emit("overlay:sound", sound);
            let _ = app.emit("overlay:drag_drop", serde_json::json!({ "id": id }));

            if let Some(id) = id {
                // One OLE drop can be reported twice; ignore repeats.
                let now = now_ms();
                if now.saturating_sub(st.last_exec_ms.swap(now, Ordering::SeqCst)) < 400 {
                    return;
                }
                if let Some(q) = st.jobs.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    q.push(Job { id, paths });
                }
            }
        }
        _ => {}
    }
}

fn on_hook_event(app: &AppHandle, event: hook::HookEvent) {
    let st = state(app);
    match event {
        hook::HookEvent::OpenWheel { mode, x, y, fresh } => {
            if fresh {
                st.overlay_gen.fetch_add(1, Ordering::SeqCst);
                let pos = window_manager::place_and_show_overlay(app, x, y);
                *st.overlay_pos.lock().unwrap_or_else(|e| e.into_inner()) = pos;
                *st.last_cursor.lock().unwrap_or_else(|e| e.into_inner()) = (180.0, 180.0);
            }
            let (items, paths, file_name, thumb, category) = {
                let mut d = st.drag.lock().unwrap_or_else(|e| e.into_inner());
                d.mode = mode.clone();
                let items = wheel::items_for(&st.config, &mode, &d.paths);
                let paths = d.paths.clone();
                let first_path = paths.first().cloned();
                let file_name = first_path.as_ref().and_then(|p| {
                    Path::new(p).file_name().map(|n| n.to_string_lossy().to_string())
                });
                let thumb = first_path.as_ref().and_then(|p| make_thumbnail(p));
                let category = if paths.is_empty() { None } else { Some(wheel::category_for(&paths)) };
                (items, paths, file_name, thumb, category)
            };
            let _ = app.emit(
                "overlay:show",
                serde_json::json!({
                    "mode": mode,
                    "fresh": fresh,
                    "items": items,
                    "paths": paths,
                    "file_name": file_name,
                    "thumb": thumb,
                    "category": category,
                }),
            );
        }
        hook::HookEvent::CursorMove { x, y } => {
            let (win_x, win_y) = *st.overlay_pos.lock().unwrap_or_else(|e| e.into_inner());
            let scale = app
                .get_webview_window("overlay")
                .and_then(|w| w.scale_factor().ok())
                .unwrap_or(1.0);
            let lx = (x - win_x) as f64 / scale;
            let ly = (y - win_y) as f64 / scale;
            *st.last_cursor.lock().unwrap_or_else(|e| e.into_inner()) = (lx, ly);
            let _ = app.emit("overlay:drag_over", serde_json::json!({ "x": lx, "y": ly }));
        }
        hook::HookEvent::ScheduleHide => {
            let _ = app.emit("overlay:close", ());
            let generation = st.overlay_gen.load(Ordering::SeqCst);
            let h = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(350));
                if state(&h).overlay_gen.load(Ordering::SeqCst) == generation {
                    window_manager::hide_overlay(&h);
                }
            });
        }
        hook::HookEvent::HideNow => {
            let _ = app.emit("overlay:close", ());
            st.overlay_gen.fetch_add(1, Ordering::SeqCst);
            window_manager::hide_overlay(app);
        }
    }
}

fn new_state() -> AppState {
    AppState {
        drag: Mutex::new(DragState { mode: "convert".into(), ..Default::default() }),
        config: config::get_wheel_config(),
        jobs: Mutex::new(None),
        overlay_gen: AtomicU64::new(0),
        hud_gen: AtomicU64::new(0),
        last_exec_ms: AtomicU64::new(0),
        hud_reveal: Mutex::new(None),
        crop_reply: Mutex::new(None),
        crop_request: Mutex::new(None),
        overlay_pos: Mutex::new((0, 0)),
        last_cursor: Mutex::new((180.0, 180.0)),
        first_run_pending: AtomicBool::new(false),
    }
}

/// The commands bridge.js can call.
fn commands<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![get_settings, overlay_revealed, crop_done, get_crop_request, hud_reveal, get_first_run, first_run_done]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(new_state())
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.handle().plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build())?;
            let handle = app.handle().clone();
            let s = settings::load_settings();
            let (_tray, autostart_item) = tray::create_tray(&handle)?;
            app.manage(tray::AutostartItem(autostart_item));

            if !s.onboarded {
                state(&handle).first_run_pending.store(true, Ordering::SeqCst);
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    if state(&h).first_run_pending.load(Ordering::SeqCst) {
                        window_manager::place_and_show_hud(&h, window_manager::FIRST_RUN_SIZE);
                        let _ = h.emit("hud:first_run", first_run_payload());
                    }
                });
            }

            *state(&handle).jobs.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(JobQueue::start(Arc::new(TauriUi { app: handle.clone() })));

            if let Some(overlay) = app.get_webview_window("overlay") {
                let h = handle.clone();
                let win = overlay.clone();
                overlay.on_window_event(move |event| {
                    if let tauri::WindowEvent::DragDrop(dde) = event {
                        on_drag_drop(&h, win.scale_factor().unwrap_or(1.0), dde);
                    }
                });
            }

            if let Some(crop) = app.get_webview_window("crop") {
                let h = handle.clone();
                crop.on_window_event(move |event| {
                    // The title-bar close button cancels the crop instead of destroying the window.
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        answer_crop(&h, None);
                    }
                });
            }

            let h = handle.clone();
            hook::start_hook(s.enabled, move |event| on_hook_event(&h, event));
            Ok(())
        })
        .invoke_handler(commands())
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

#[cfg(test)]
mod e2e;
