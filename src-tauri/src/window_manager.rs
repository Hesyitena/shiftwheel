use tauri::{AppHandle, Manager};
use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};

pub const OVERLAY_W: i32 = 360;
pub const OVERLAY_H: i32 = 380;
pub const WHEEL_CENTER: i32 = 180;

pub const HUD_W: i32 = 340;
pub const HUD_H: i32 = 92;

pub fn get_work_area_for_point(pt: POINT) -> RECT {
    unsafe {
        let h_mon: HMONITOR = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(h_mon, &mut mi) != 0 {
            mi.rcWork
        } else {
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            }
        }
    }
}

pub fn place_and_show_overlay(app: &AppHandle, x: i32, y: i32) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let scale = overlay.scale_factor().unwrap_or(1.0);
        let pt = POINT { x, y };
        let wa = get_work_area_for_point(pt);

        let target_x = (x - WHEEL_CENTER).clamp(wa.left, wa.right - OVERLAY_W);
        let target_y = (y - WHEEL_CENTER).clamp(wa.top, wa.bottom - OVERLAY_H);

        let logical_x = target_x as f64 / scale;
        let logical_y = target_y as f64 / scale;

        let _ = overlay.set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: logical_x,
            y: logical_y,
        }));
        let _ = overlay.set_always_on_top(true);
        let _ = overlay.show();
    }
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
}

pub fn place_and_show_hud(app: &AppHandle) {
    if let Some(hud) = app.get_webview_window("hud") {
        if let Ok(Some(mon)) = hud.current_monitor() {
            let mon_size = mon.size();
            let scale = mon.scale_factor();
            let logical_w = mon_size.width as f64 / scale;
            let logical_h = mon_size.height as f64 / scale;
            let target_x = logical_w - (HUD_W as f64) - 16.0;
            let target_y = logical_h - (HUD_H as f64) - 16.0;

            let _ = hud.set_position(tauri::Position::Logical(tauri::LogicalPosition {
                x: target_x,
                y: target_y,
            }));
        }
        let _ = hud.set_always_on_top(true);
        let _ = hud.show();
    }
}

pub fn hide_hud(app: &AppHandle) {
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.hide();
    }
}
