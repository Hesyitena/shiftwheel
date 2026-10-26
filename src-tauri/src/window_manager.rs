use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    SW_SHOWNOACTIVATE,
};

pub const OVERLAY_W: i32 = 360;
pub const OVERLAY_H: i32 = 380;
pub const WHEEL_CENTER: i32 = 180;

pub const HUD_W: i32 = 340;
pub const HUD_H: i32 = 92;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn get_overlay_hwnd() -> HWND {
    unsafe {
        let title = to_wide("Shiftwheel");
        FindWindowW(std::ptr::null(), title.as_ptr())
    }
}

pub fn get_hud_hwnd() -> HWND {
    unsafe {
        let title = to_wide("Shiftwheel HUD");
        FindWindowW(std::ptr::null(), title.as_ptr())
    }
}

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

pub fn place_and_show_overlay(x: i32, y: i32) {
    let pt = POINT { x, y };
    let wa = get_work_area_for_point(pt);

    let clamp = |val: i32, min: i32, max: i32| -> i32 {
        if val < min {
            min
        } else if val > max {
            max
        } else {
            val
        }
    };

    let target_x = clamp(x - WHEEL_CENTER, wa.left, wa.right - OVERLAY_W);
    let target_y = clamp(y - WHEEL_CENTER, wa.top, wa.bottom - OVERLAY_H);

    let hwnd = get_overlay_hwnd();
    if hwnd != 0 as HWND {
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                target_x,
                target_y,
                OVERLAY_W,
                OVERLAY_H,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
}

pub fn hide_overlay() {
    let hwnd = get_overlay_hwnd();
    if hwnd != 0 as HWND {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

pub fn place_and_show_hud(x: i32, y: i32) {
    let pt = POINT { x, y };
    let wa = get_work_area_for_point(pt);

    let target_x = wa.right - HUD_W - 8;
    let target_y = wa.bottom - HUD_H - 4;

    let hwnd = get_hud_hwnd();
    if hwnd != 0 as HWND {
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                target_x,
                target_y,
                HUD_W,
                HUD_H,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
}

pub fn hide_hud() {
    let hwnd = get_hud_hwnd();
    if hwnd != 0 as HWND {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}
