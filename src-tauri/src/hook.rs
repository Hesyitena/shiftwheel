use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

const DRAG_THRESHOLD: f64 = 4.0;
const REPEAT_GAP: Duration = Duration::from_millis(600);

#[derive(Debug, Clone)]
pub enum HookEvent {
    OpenWheel { mode: String, x: i32, y: i32 },
    ScheduleHide,
    HideNow,
    ProbeCheck,
}

static ENABLED: AtomicBool = AtomicBool::new(true);
static SHOWN: AtomicBool = AtomicBool::new(false);

struct State {
    down: bool,
    moved: bool,
    x0: i32,
    y0: i32,
    last_move: Instant,
    pending_mode: Option<String>,
    pre_tap: Option<(String, Instant)>,
    last_shift_down: Option<Instant>,
    last_ctrl_down: Option<Instant>,
    last_alt_down: Option<Instant>,
    event_callback: Option<Box<dyn Fn(HookEvent) + Send + Sync + 'static>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled {
        reset_drag();
    }
}

pub fn set_shown(shown: bool) {
    SHOWN.store(shown, Ordering::SeqCst);
}

pub fn reset_drag() {
    if let Ok(mut lock) = STATE.lock() {
        if let Some(state) = lock.as_mut() {
            state.down = false;
            state.moved = false;
            state.pending_mode = None;
            state.pre_tap = None;
        }
    }
}

fn emit(event: HookEvent) {
    if let Ok(lock) = STATE.lock() {
        if let Some(state) = lock.as_ref() {
            if let Some(cb) = &state.event_callback {
                cb(event);
            }
        }
    }
}

fn mode_for_vk(vk: u16) -> Option<&'static str> {
    match vk {
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some("convert"),
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => Some("tools"),
        VK_MENU | VK_LMENU | VK_RMENU => Some("actions"),
        _ => None,
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && ENABLED.load(Ordering::SeqCst) {
        let msg = wparam as u32;
        let hook_struct = unsafe { *(lparam as *const MSLLHOOKSTRUCT) };
        let pt = hook_struct.pt;
        let now = Instant::now();

        match msg {
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                let mut should_hide = false;
                if let Ok(mut lock) = STATE.lock() {
                    if let Some(state) = lock.as_mut() {
                        should_hide = SHOWN.load(Ordering::SeqCst);
                        state.down = true;
                        state.moved = false;
                        state.x0 = pt.x;
                        state.y0 = pt.y;
                        state.last_move = now;
                        state.pending_mode = None;
                    }
                }
                if should_hide {
                    emit(HookEvent::HideNow);
                }
            }
            WM_MOUSEMOVE => {
                let mut to_open: Option<(String, i32, i32)> = None;
                if let Ok(mut lock) = STATE.lock() {
                    if let Some(state) = lock.as_mut() {
                        state.last_move = now;
                        if state.down {
                            let dist = (((pt.x - state.x0).pow(2) + (pt.y - state.y0).pow(2)) as f64).sqrt();
                            if !state.moved && dist >= DRAG_THRESHOLD {
                                state.moved = true;
                                if let Some(m) = state.pending_mode.take() {
                                    to_open = Some((m, pt.x, pt.y));
                                }
                            }
                        }
                    }
                }
                if let Some((mode, x, y)) = to_open {
                    emit(HookEvent::OpenWheel { mode, x, y });
                }
                if SHOWN.load(Ordering::SeqCst) {
                    emit(HookEvent::ProbeCheck);
                }
            }
            WM_LBUTTONUP | WM_RBUTTONUP => {
                let mut was_down = false;
                if let Ok(mut lock) = STATE.lock() {
                    if let Some(state) = lock.as_mut() {
                        was_down = state.down;
                        state.down = false;
                        state.moved = false;
                        state.pending_mode = None;
                        state.pre_tap = None;
                    }
                }
                if was_down || SHOWN.load(Ordering::SeqCst) {
                    emit(HookEvent::ScheduleHide);
                }
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(0 as HHOOK, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && ENABLED.load(Ordering::SeqCst) {
        let msg = wparam as u32;
        let hook_struct = unsafe { *(lparam as *const KBDLLHOOKSTRUCT) };
        let vk = hook_struct.vkCode as u16;
        let now = Instant::now();

        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            if vk == VK_ESCAPE {
                reset_drag();
                if SHOWN.load(Ordering::SeqCst) {
                    emit(HookEvent::ScheduleHide);
                } else {
                    emit(HookEvent::HideNow);
                }
                return unsafe { CallNextHookEx(0 as HHOOK, code, wparam, lparam) };
            }

            if let Some(mode) = mode_for_vk(vk) {
                let mut is_fresh = false;
                let mut to_open: Option<(String, i32, i32)> = None;

                if let Ok(mut lock) = STATE.lock() {
                    if let Some(state) = lock.as_mut() {
                        let last = match mode {
                            "convert" => &mut state.last_shift_down,
                            "tools" => &mut state.last_ctrl_down,
                            "actions" => &mut state.last_alt_down,
                            _ => unreachable!(),
                        };

                        if last.map_or(true, |t| now.duration_since(t) >= REPEAT_GAP) {
                            *last = Some(now);
                            is_fresh = true;
                        }

                        if is_fresh {
                            if state.down {
                                state.pre_tap = None;
                                if state.moved {
                                    let mut pt: POINT = unsafe { std::mem::zeroed() };
                                    unsafe {
                                        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                                    }
                                    to_open = Some((mode.to_string(), pt.x, pt.y));
                                } else {
                                    state.pending_mode = Some(mode.to_string());
                                }
                            } else {
                                state.pre_tap = Some((mode.to_string(), now));
                            }
                        }
                    }
                }

                if let Some((m, x, y)) = to_open {
                    emit(HookEvent::OpenWheel { mode: m, x, y });
                }
            }
        } else if msg == WM_KEYUP || msg == WM_SYSKEYUP {
            if let Some(mode) = mode_for_vk(vk) {
                let mut to_open: Option<(String, i32, i32)> = None;
                if let Ok(mut lock) = STATE.lock() {
                    if let Some(state) = lock.as_mut() {
                        match mode {
                            "convert" => state.last_shift_down = None,
                            "tools" => state.last_ctrl_down = None,
                            "actions" => state.last_alt_down = None,
                            _ => {}
                        }

                        if state.down && !SHOWN.load(Ordering::SeqCst) {
                            if let Some((pre_mode, pre_time)) = &state.pre_tap {
                                if pre_mode == mode && now.duration_since(*pre_time) < Duration::from_millis(450) {
                                    state.pre_tap = None;
                                    state.moved = true;
                                    let mut pt: POINT = unsafe { std::mem::zeroed() };
                                    unsafe {
                                        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                                    }
                                    to_open = Some((mode.to_string(), pt.x, pt.y));
                                }
                            }
                        }
                    }
                }
                if let Some((m, x, y)) = to_open {
                    emit(HookEvent::OpenWheel { mode: m, x, y });
                }
            }
        }
    }
    unsafe { CallNextHookEx(0 as HHOOK, code, wparam, lparam) }
}

pub fn start_hook<F>(callback: F)
where
    F: Fn(HookEvent) + Send + Sync + 'static,
{
    {
        let mut lock = STATE.lock().unwrap();
        *lock = Some(State {
            down: false,
            moved: false,
            x0: 0,
            y0: 0,
            last_move: Instant::now(),
            pending_mode: None,
            pre_tap: None,
            last_shift_down: None,
            last_ctrl_down: None,
            last_alt_down: None,
            event_callback: Some(Box::new(callback)),
        });
    }

    std::thread::spawn(|| unsafe {
        let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), 0 as _, 0);
        let kbd_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), 0 as _, 0);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        UnhookWindowsHookEx(mouse_hook);
        UnhookWindowsHookEx(kbd_hook);
    });
}
