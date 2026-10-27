//! Placing the overlay and HUD windows.
//!
//! All math is in physical pixels using the DPI of the monitor the window is going to, so the
//! wheel stays centered on the cursor at 125 % / 150 % scaling and on mixed-DPI setups.

use tauri::{AppHandle, Manager, Runtime};

pub const OVERLAY_W: f64 = 360.0;
pub const OVERLAY_H: f64 = 380.0;
pub const WHEEL_CENTER: f64 = 180.0;
/// The progress card window (logical px, including room for the card's shadow).
pub const HUD_SIZE: (f64, f64) = (372.0, 150.0);
/// The same window while it shows the first-run card.
pub const FIRST_RUN_SIZE: (f64, f64) = (400.0, 330.0);
/// HUD distance from the work-area corner (logical px). The card already has 12-14 px of
/// transparent padding for its shadow.
const HUD_MARGIN: f64 = 4.0;

/// A monitor work area (excludes the taskbar) in physical pixels, plus its scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub scale: f64,
}

/// Top-left (physical) for a window of logical size `w`×`h` whose logical point (`ax`, `ay`)
/// should sit on (`x`, `y`), kept inside the work area.
pub fn anchored_position(wa: WorkArea, x: i32, y: i32, w: f64, h: f64, ax: f64, ay: f64) -> (i32, i32) {
    let (pw, ph) = ((w * wa.scale).round() as i32, (h * wa.scale).round() as i32);
    let tx = x - (ax * wa.scale).round() as i32;
    let ty = y - (ay * wa.scale).round() as i32;
    let clamp = |v: i32, lo: i32, hi: i32| if hi < lo { lo } else { v.clamp(lo, hi) };
    (clamp(tx, wa.left, wa.right - pw), clamp(ty, wa.top, wa.bottom - ph))
}

/// Bottom-right corner position (physical) for a HUD window of logical size `size`.
pub fn hud_position(wa: WorkArea, size: (f64, f64)) -> (i32, i32) {
    let m = (HUD_MARGIN * wa.scale).round() as i32;
    (
        wa.right - (size.0 * wa.scale).round() as i32 - m,
        wa.bottom - (size.1 * wa.scale).round() as i32 - m,
    )
}

#[cfg(windows)]
pub fn work_area_at(x: i32, y: i32) -> WorkArea {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let (mut dx, mut dy) = (96u32, 96u32);
        let scale = if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0 { dx as f64 / 96.0 } else { 1.0 };
        if GetMonitorInfoW(mon, &mut mi) != 0 {
            let r = mi.rcWork;
            WorkArea { left: r.left, top: r.top, right: r.right, bottom: r.bottom, scale }
        } else {
            WorkArea { left: 0, top: 0, right: 1920, bottom: 1080, scale }
        }
    }
}

#[cfg(windows)]
fn cursor() -> (i32, i32) {
    use windows_sys::Win32::Foundation::POINT;
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) };
    (pt.x, pt.y)
}

/// Non-Windows builds (development only): use Tauri's monitor info.
#[cfg(not(windows))]
pub fn work_area_at(_x: i32, _y: i32) -> WorkArea {
    WorkArea { left: 0, top: 0, right: 1920, bottom: 1080, scale: 1.0 }
}

#[cfg(not(windows))]
fn cursor() -> (i32, i32) {
    (0, 0)
}

fn set_physical<R: Runtime>(win: &tauri::WebviewWindow<R>, (x, y): (i32, i32)) {
    let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
}

pub fn place_and_show_overlay<R: Runtime>(app: &AppHandle<R>, x: i32, y: i32) -> (i32, i32) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let wa = work_area_at(x, y);
        let pos = anchored_position(wa, x, y, OVERLAY_W, OVERLAY_H, WHEEL_CENTER, WHEEL_CENTER);
        set_physical(&overlay, pos);
        let _ = overlay.show();
        // Moving to a monitor with another DPI resizes the window after the move; place it again.
        set_physical(&overlay, pos);
        pos
    } else {
        (x - WHEEL_CENTER as i32, y - WHEEL_CENTER as i32)
    }
}

pub fn hide_overlay<R: Runtime>(app: &AppHandle<R>) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
}

pub fn place_and_show_hud<R: Runtime>(app: &AppHandle<R>, size: (f64, f64)) {
    if let Some(hud) = app.get_webview_window("hud") {
        let (x, y) = cursor();
        let pos = hud_position(work_area_at(x, y), size);
        let _ = hud.set_size(tauri::Size::Logical(tauri::LogicalSize { width: size.0, height: size.1 }));
        set_physical(&hud, pos);
        let _ = hud.show();
        set_physical(&hud, pos);
    }
}

pub fn hide_hud<R: Runtime>(app: &AppHandle<R>) {
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_centers_on_cursor_at_150_percent() {
        let wa = WorkArea { left: 0, top: 0, right: 2880, bottom: 1800, scale: 1.5 };
        let (x, y) = anchored_position(wa, 1000, 900, OVERLAY_W, OVERLAY_H, WHEEL_CENTER, WHEEL_CENTER);
        // window top-left + 180 logical px * 1.5 = cursor
        assert_eq!((x + 270, y + 270), (1000, 900));
    }

    #[test]
    fn wheel_stays_on_screen_at_edges() {
        let wa = WorkArea { left: 0, top: 0, right: 2880, bottom: 1752, scale: 1.5 };
        let (x, y) = anchored_position(wa, 2870, 1750, OVERLAY_W, OVERLAY_H, WHEEL_CENTER, WHEEL_CENTER);
        assert_eq!(x + 540, 2880);
        assert_eq!(y + 570, 1752);
    }

    #[test]
    fn secondary_monitor_left_of_primary() {
        let wa = WorkArea { left: -1920, top: 0, right: 0, bottom: 1040, scale: 1.0 };
        let (x, y) = anchored_position(wa, -1000, 500, OVERLAY_W, OVERLAY_H, WHEEL_CENTER, WHEEL_CENTER);
        assert_eq!((x, y), (-1180, 320));
    }

    #[cfg(windows)]
    #[test]
    fn real_work_area_is_sane() {
        let wa = work_area_at(10, 10);
        assert!(wa.right > wa.left && wa.bottom > wa.top, "{wa:?}");
        assert!(wa.scale >= 1.0 && wa.scale <= 4.0, "{wa:?}");
    }

    #[test]
    fn hud_sits_above_taskbar() {
        // 1080p at 125 % with a 60 px taskbar: work area bottom 1020.
        let wa = WorkArea { left: 0, top: 0, right: 1920, bottom: 1020, scale: 1.25 };
        let (x, y) = hud_position(wa, HUD_SIZE);
        assert_eq!(x + (HUD_SIZE.0 * 1.25) as i32 + 5, 1920);
        assert_eq!(y + (HUD_SIZE.1 * 1.25).round() as i32 + 5, 1020);
        let (_, y2) = hud_position(wa, FIRST_RUN_SIZE);
        assert!(y2 < y, "the bigger card grows upwards");
    }

}
