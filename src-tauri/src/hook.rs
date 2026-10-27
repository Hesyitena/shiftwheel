#![cfg_attr(not(windows), allow(dead_code))]
//! Global mouse/keyboard hook. The decision logic lives in [`Machine`] (plain Rust, testable);
//! the Win32 glue only feeds it input and forwards the resulting events through a channel, so
//! the hook callback never blocks (Windows silently removes hooks that are too slow).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

const DRAG_THRESHOLD: i32 = 4;
/// "Tap the key, then start dragging" window.
const PRE_TAP_WINDOW: Duration = Duration::from_millis(450);
/// A held key repeats well within this (the longest Windows repeat delay is 1 s). A key marked
/// held with no event for longer lost its key-up, so its next press counts as new.
const STUCK_KEY: Duration = Duration::from_millis(1200);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookEvent {
    /// Show the wheel. `fresh` is false when switching mode on a wheel that is already open
    /// (then the window must stay where it is).
    OpenWheel { mode: String, x: i32, y: i32, fresh: bool },
    /// Track mouse cursor position while wheel is visible so slices can highlight and cancel text appears
    CursorMove { x: i32, y: i32 },
    /// Mouse released: fade out, then hide.
    ScheduleHide,
    /// Hide at once (click elsewhere).
    HideNow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Convert,
    Tools,
    Actions,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Convert => "convert",
            Mode::Tools => "tools",
            Mode::Actions => "actions",
        }
    }
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Input {
    MouseDown { x: i32, y: i32 },
    MouseMove { x: i32, y: i32 },
    MouseUp,
    KeyDown(Mode),
    KeyUp(Mode),
    Escape,
}

#[derive(Debug, Default)]
pub struct Machine {
    pub enabled: bool,
    pub shown: bool,
    down: bool,
    moved: bool,
    x0: i32,
    y0: i32,
    pending_mode: Option<Mode>,
    pre_tap: Option<(Mode, Instant)>,
    last_key_down: [Option<Instant>; 3],
    last_tap: [Option<Instant>; 3],
    key_held: [bool; 3],
}

impl Machine {
    pub fn new() -> Self {
        Self { enabled: true, ..Default::default() }
    }

    pub fn reset_drag(&mut self) {
        self.down = false;
        self.moved = false;
        self.pending_mode = None;
        self.pre_tap = None;
        self.key_held = [false; 3];
    }

    fn open(&mut self, mode: Mode, cursor: (i32, i32)) -> HookEvent {
        let fresh = !self.shown;
        self.shown = true;
        HookEvent::OpenWheel { mode: mode.name().into(), x: cursor.0, y: cursor.1, fresh }
    }

    /// Feed one input; `cursor` is the current pointer position (screen pixels).
    pub fn handle(&mut self, input: Input, now: Instant, cursor: (i32, i32)) -> Option<HookEvent> {
        if !self.enabled {
            return None;
        }
        match input {
            Input::MouseDown { x, y } => {
                let was_shown = self.shown;
                self.down = true;
                self.moved = false;
                self.x0 = x;
                self.y0 = y;
                self.pending_mode = None;
                if was_shown {
                    self.shown = false;
                    return Some(HookEvent::HideNow);
                }
                None
            }
            Input::MouseMove { x, y } => {
                if self.shown {
                    return Some(HookEvent::CursorMove { x, y });
                }
                if self.down && !self.moved {
                    let (dx, dy) = ((x - self.x0) as i64, (y - self.y0) as i64);
                    if dx * dx + dy * dy >= (DRAG_THRESHOLD * DRAG_THRESHOLD) as i64 {
                        self.moved = true;
                        if let Some(mode) = self.pending_mode.take() {
                            return Some(self.open(mode, (x, y)));
                        }
                    }
                }
                None
            }
            Input::MouseUp => {
                let was_down = self.down;
                self.reset_drag();
                if self.shown {
                    self.shown = false;
                    return Some(HookEvent::ScheduleHide);
                }
                let _ = was_down;
                None
            }
            Input::Escape => {
                self.reset_drag();
                if self.shown {
                    self.shown = false;
                    return Some(HookEvent::ScheduleHide);
                }
                None
            }
            Input::KeyDown(mode) => {
                // Auto-repeat while the key is held is ignored; a key-up that never came is not.
                let i = mode.index();
                let repeat = self.key_held[i] && self.last_key_down[i].is_some_and(|t| now.duration_since(t) < STUCK_KEY);
                self.last_key_down[i] = Some(now);
                if repeat {
                    return None;
                }
                self.key_held[i] = true;
                if self.down {
                    self.pre_tap = None;
                    if self.moved {
                        return Some(self.open(mode, cursor));
                    }
                    self.pending_mode = Some(mode);
                } else {
                    // Double-tap shortcut to open/preview wheel without dragging
                    if let Some(t) = self.last_tap[mode.index()] {
                        if now.duration_since(t) < Duration::from_millis(400) {
                            self.last_tap[mode.index()] = None;
                            self.pre_tap = None;
                            return Some(self.open(mode, cursor));
                        }
                    }
                    self.last_tap[mode.index()] = Some(now);
                    self.pre_tap = Some((mode, now));
                }
                None
            }
            Input::KeyUp(mode) => {
                self.key_held[mode.index()] = false;
                self.last_key_down[mode.index()] = None;
                if self.down && !self.shown {
                    if let Some((pre_mode, at)) = self.pre_tap {
                        if pre_mode == mode && now.duration_since(at) < PRE_TAP_WINDOW {
                            self.pre_tap = None;
                            self.moved = true;
                            return Some(self.open(mode, cursor));
                        }
                    }
                }
                None
            }
        }
    }
}

static MACHINE: Mutex<Option<Machine>> = Mutex::new(None);

fn with_machine<R>(f: impl FnOnce(&mut Machine) -> R) -> Option<R> {
    let mut lock = MACHINE.lock().unwrap_or_else(|e| e.into_inner());
    lock.as_mut().map(f)
}

pub fn set_enabled(enabled: bool) {
    with_machine(|m| {
        m.enabled = enabled;
        if !enabled {
            m.reset_drag();
            m.shown = false;
        }
    });
}

pub fn set_shown(shown: bool) {
    with_machine(|m| m.shown = shown);
}

pub fn reset_drag() {
    with_machine(Machine::reset_drag);
}

/// Feed input from any source (the Win32 hook, or tests).
pub fn feed(input: Input, cursor: (i32, i32)) -> Option<HookEvent> {
    with_machine(|m| m.handle(input, Instant::now(), cursor)).flatten()
}

/// Hands events from the hook to the app thread. Mouse moves are merged: at most one waits in
/// the queue and it carries the latest position when handled. Without this, a fast mouse
/// queues moves faster than the app handles them, and a mode switch (Shift / Ctrl / Alt) waits
/// behind that growing backlog, so switching seemed not to work after a few uses.
#[derive(Default)]
pub struct Relay {
    move_pending: AtomicBool,
    latest: AtomicU64,
}

impl Relay {
    pub const fn new() -> Self {
        Self { move_pending: AtomicBool::new(false), latest: AtomicU64::new(0) }
    }

    /// Hook side: queue `ev` (never blocks).
    pub fn post(&self, tx: &Sender<HookEvent>, ev: HookEvent) {
        if let HookEvent::CursorMove { x, y } = ev {
            self.latest.store(((x as u32 as u64) << 32) | y as u32 as u64, Ordering::SeqCst);
            if self.move_pending.swap(true, Ordering::SeqCst) {
                return; // one is already waiting; it will pick up this position
            }
        }
        let _ = tx.send(ev);
    }

    /// App side: the event to handle, with a merged move resolved to the newest position.
    pub fn take(&self, ev: HookEvent) -> HookEvent {
        match ev {
            HookEvent::CursorMove { .. } => {
                self.move_pending.store(false, Ordering::SeqCst);
                let v = self.latest.load(Ordering::SeqCst);
                HookEvent::CursorMove { x: (v >> 32) as u32 as i32, y: v as u32 as i32 }
            }
            other => other,
        }
    }

    /// Run `callback` for every event until the sender is gone.
    pub fn dispatch(&self, rx: Receiver<HookEvent>, callback: impl Fn(HookEvent)) {
        for ev in rx {
            callback(self.take(ev));
        }
    }
}

static RELAY: Relay = Relay::new();

/// Install the hook. `callback` runs on its own thread, never inside the hook procedure.
pub fn start_hook<F>(enabled: bool, callback: F)
where
    F: Fn(HookEvent) + Send + 'static,
{
    {
        let mut m = Machine::new();
        m.enabled = enabled;
        *MACHINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(m);
    }
    let (tx, rx) = std::sync::mpsc::channel::<HookEvent>();
    std::thread::Builder::new()
        .name("shiftwheel-hook-events".into())
        .spawn(move || RELAY.dispatch(rx, callback))
        .expect("hook event thread");
    #[cfg(windows)]
    win::install(tx);
    #[cfg(not(windows))]
    drop(tx);
}

#[cfg(windows)]
mod win {
    use super::{Input, Mode, feed};
    use std::sync::Mutex;
    use std::sync::mpsc::Sender;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetCursorPos, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    static TX: Mutex<Option<Sender<super::HookEvent>>> = Mutex::new(None);

    fn send(ev: Option<super::HookEvent>) {
        if let Some(ev) = ev {
            if let Some(tx) = TX.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                super::RELAY.post(tx, ev);
            }
        }
    }

    fn cursor() -> (i32, i32) {
        let mut pt = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut pt) };
        (pt.x, pt.y)
    }

    fn mode_for_vk(vk: u16) -> Option<Mode> {
        match vk {
            VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some(Mode::Convert),
            VK_CONTROL | VK_LCONTROL | VK_RCONTROL => Some(Mode::Tools),
            VK_MENU | VK_LMENU | VK_RMENU => Some(Mode::Actions),
            _ => None,
        }
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            let (x, y) = (info.pt.x, info.pt.y);
            let input = match wparam as u32 {
                WM_LBUTTONDOWN | WM_RBUTTONDOWN => Some(Input::MouseDown { x, y }),
                WM_MOUSEMOVE => Some(Input::MouseMove { x, y }),
                WM_LBUTTONUP | WM_RBUTTONUP => Some(Input::MouseUp),
                _ => None,
            };
            if let Some(input) = input {
                send(feed(input, (x, y)));
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            let vk = info.vkCode as u16;
            let msg = wparam as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            let input = if vk == VK_ESCAPE && down {
                Some(Input::Escape)
            } else {
                mode_for_vk(vk).and_then(|m| {
                    if down {
                        Some(Input::KeyDown(m))
                    } else if up {
                        Some(Input::KeyUp(m))
                    } else {
                        None
                    }
                })
            };
            if let Some(input) = input {
                send(feed(input, cursor()));
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    pub fn install(tx: Sender<super::HookEvent>) {
        *TX.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        // One pair of hooks per process; a second start only redirects the events (tests).
        static INSTALLED: std::sync::Once = std::sync::Once::new();
        INSTALLED.call_once(start_thread);
    }

    fn start_thread() {
        std::thread::Builder::new()
            .name("shiftwheel-hook".into())
            .spawn(|| unsafe {
                let mouse: HHOOK = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), std::ptr::null_mut(), 0);
                let kbd: HHOOK = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), std::ptr::null_mut(), 0);
                if mouse.is_null() || kbd.is_null() {
                    log::error!("Could not install the input hooks");
                }
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                UnhookWindowsHookEx(mouse);
                UnhookWindowsHookEx(kbd);
            })
            .expect("hook thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that touch the global hook state run one at a time.
    static GLOBAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn drag_then_tap_opens_fresh_wheel() {
        let mut m = Machine::new();
        let t = t0();
        assert_eq!(m.handle(Input::MouseDown { x: 10, y: 10 }, t, (10, 10)), None);
        assert_eq!(m.handle(Input::MouseMove { x: 30, y: 10 }, t, (30, 10)), None);
        let ev = m.handle(Input::KeyDown(Mode::Convert), t, (30, 10));
        assert_eq!(ev, Some(HookEvent::OpenWheel { mode: "convert".into(), x: 30, y: 10, fresh: true }));
        assert!(m.shown);
    }

    #[test]
    fn switching_mode_is_not_fresh() {
        let mut m = Machine::new();
        let t = t0();
        m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0));
        m.handle(Input::MouseMove { x: 20, y: 0 }, t, (20, 0));
        m.handle(Input::KeyDown(Mode::Convert), t, (20, 0));
        let ev = m.handle(Input::KeyDown(Mode::Tools), t, (50, 50));
        assert_eq!(ev, Some(HookEvent::OpenWheel { mode: "tools".into(), x: 50, y: 50, fresh: false }));
    }

    #[test]
    fn tap_before_drag_threshold_opens_on_move() {
        let mut m = Machine::new();
        let t = t0();
        m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0));
        assert_eq!(m.handle(Input::KeyDown(Mode::Actions), t, (0, 0)), None);
        assert_eq!(m.handle(Input::MouseMove { x: 2, y: 0 }, t, (2, 0)), None);
        let ev = m.handle(Input::MouseMove { x: 5, y: 0 }, t, (5, 0));
        assert!(matches!(ev, Some(HookEvent::OpenWheel { fresh: true, .. })));
    }

    #[test]
    fn key_repeat_is_ignored_and_release_hides() {
        let mut m = Machine::new();
        let t = t0();
        m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0));
        m.handle(Input::MouseMove { x: 9, y: 0 }, t, (9, 0));
        assert!(m.handle(Input::KeyDown(Mode::Convert), t, (9, 0)).is_some());
        assert_eq!(m.handle(Input::KeyDown(Mode::Convert), t + Duration::from_millis(30), (9, 0)), None);
        assert_eq!(m.handle(Input::MouseUp, t, (9, 0)), Some(HookEvent::ScheduleHide));
        assert!(!m.shown);
        // A plain mouse-up without a wheel sends nothing.
        assert_eq!(m.handle(Input::MouseUp, t, (9, 0)), None);
    }

    #[test]
    fn escape_and_click_close() {
        let mut m = Machine::new();
        let t = t0();
        m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0));
        m.handle(Input::MouseMove { x: 9, y: 0 }, t, (9, 0));
        m.handle(Input::KeyDown(Mode::Convert), t, (9, 0));
        assert_eq!(m.handle(Input::Escape, t, (9, 0)), Some(HookEvent::ScheduleHide));
        assert_eq!(m.handle(Input::Escape, t, (9, 0)), None);
        m.shown = true;
        assert_eq!(m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0)), Some(HookEvent::HideNow));
    }

    #[test]
    fn double_tap_without_drag_opens_wheel() {
        let mut m = Machine::new();
        let t = t0();
        assert_eq!(m.handle(Input::KeyDown(Mode::Convert), t, (15, 15)), None);
        assert_eq!(m.handle(Input::KeyUp(Mode::Convert), t + Duration::from_millis(50), (15, 15)), None);
        let ev = m.handle(Input::KeyDown(Mode::Convert), t + Duration::from_millis(100), (15, 15));
        assert_eq!(ev, Some(HookEvent::OpenWheel { mode: "convert".into(), x: 15, y: 15, fresh: true }));
        assert!(m.shown);
    }

    /// Bug: after a few uses the wheel still opened but Shift / Ctrl / Alt no longer switched it.
    #[test]
    fn switching_works_on_every_use() {
        let mut m = Machine::new();
        let mut t = t0();
        let ms = Duration::from_millis;
        for round in 0..20 {
            m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0));
            m.handle(Input::MouseMove { x: 20, y: 0 }, t, (20, 0));
            let open = m.handle(Input::KeyDown(Mode::Convert), t, (20, 0));
            assert!(matches!(open, Some(HookEvent::OpenWheel { fresh: true, .. })), "round {round}: {open:?}");
            m.handle(Input::KeyUp(Mode::Convert), t + ms(60), (20, 0));
            for (i, mode) in [Mode::Tools, Mode::Actions, Mode::Convert].into_iter().enumerate() {
                let at = t + ms(200 + 150 * i as u64);
                let ev = m.handle(Input::KeyDown(mode), at, (20, 0));
                assert_eq!(ev, Some(HookEvent::OpenWheel { mode: mode.name().into(), x: 20, y: 0, fresh: false }), "round {round}");
                m.handle(Input::KeyUp(mode), at + ms(60), (20, 0));
            }
            assert_eq!(m.handle(Input::MouseUp, t + ms(800), (20, 0)), Some(HookEvent::ScheduleHide));
            t += ms(2000);
        }
    }

    #[test]
    fn a_lost_key_up_does_not_block_the_key() {
        let mut m = Machine::new();
        let t = t0();
        m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0));
        m.handle(Input::MouseMove { x: 20, y: 0 }, t, (20, 0));
        m.handle(Input::KeyDown(Mode::Convert), t, (20, 0));
        // Ctrl goes down, its key-up never arrives (e.g. dropped by Windows)
        assert!(m.handle(Input::KeyDown(Mode::Tools), t + Duration::from_millis(100), (20, 0)).is_some());
        // auto-repeat while held is still ignored...
        assert_eq!(m.handle(Input::KeyDown(Mode::Tools), t + Duration::from_millis(600), (20, 0)), None);
        m.handle(Input::KeyDown(Mode::Convert), t + Duration::from_millis(700), (20, 0));
        // ...but a press after a quiet gap is a new press
        let ev = m.handle(Input::KeyDown(Mode::Tools), t + Duration::from_millis(2500), (20, 0));
        assert!(matches!(ev, Some(HookEvent::OpenWheel { ref mode, fresh: false, .. }) if mode == "tools"), "{ev:?}");
    }

    #[test]
    fn mouse_moves_never_pile_up_ahead_of_a_switch() {
        let relay = Relay::new();
        let (tx, rx) = std::sync::mpsc::channel();
        relay.post(&tx, HookEvent::OpenWheel { mode: "convert".into(), x: 0, y: 0, fresh: true });
        // A fast mouse while the app is busy: thousands of moves
        for i in 0..5000 {
            relay.post(&tx, HookEvent::CursorMove { x: i, y: -i });
        }
        relay.post(&tx, HookEvent::OpenWheel { mode: "tools".into(), x: 0, y: 0, fresh: false });
        drop(tx);
        let got: Vec<HookEvent> = rx.into_iter().map(|e| relay.take(e)).collect();
        assert_eq!(
            got,
            vec![
                HookEvent::OpenWheel { mode: "convert".into(), x: 0, y: 0, fresh: true },
                HookEvent::CursorMove { x: 4999, y: -4999 },
                HookEvent::OpenWheel { mode: "tools".into(), x: 0, y: 0, fresh: false },
            ]
        );
        // After it was handled, the next move is queued again
        let (tx, rx) = std::sync::mpsc::channel();
        relay.post(&tx, HookEvent::CursorMove { x: 7, y: 8 });
        assert_eq!(relay.take(rx.recv().unwrap()), HookEvent::CursorMove { x: 7, y: 8 });
    }

    #[test]
    fn disabled_does_nothing() {
        let mut m = Machine::new();
        m.enabled = false;
        let t = t0();
        m.handle(Input::MouseDown { x: 0, y: 0 }, t, (0, 0));
        m.handle(Input::MouseMove { x: 9, y: 0 }, t, (9, 0));
        assert_eq!(m.handle(Input::KeyDown(Mode::Convert), t, (9, 0)), None);
    }

    /// Drives the real low-level hooks with synthesized input (SendInput).
    #[cfg(windows)]
    #[test]
    fn real_hook_opens_and_closes_the_wheel() {
        use std::sync::mpsc::channel;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN,
            MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput, VK_LSHIFT,
        };
        fn mouse(flags: u32, dx: i32, dy: i32) -> INPUT {
            INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
        }
        fn key(vk: u16, up: bool) -> INPUT {
            let flags = if up { KEYEVENTF_KEYUP } else { 0 };
            INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
        }
        fn send(i: INPUT) {
            unsafe { SendInput(1, &i, std::mem::size_of::<INPUT>() as i32) };
            std::thread::sleep(Duration::from_millis(60));
        }

        let _g = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = channel();
        start_hook(true, move |ev| {
            let _ = tx.send(ev);
        });
        std::thread::sleep(Duration::from_millis(300));
        send(mouse(MOUSEEVENTF_LEFTDOWN, 0, 0));
        send(mouse(MOUSEEVENTF_MOVE, 20, 0));
        send(mouse(MOUSEEVENTF_MOVE, 20, 0));
        send(key(VK_LSHIFT, false));
        send(key(VK_LSHIFT, true));
        send(mouse(MOUSEEVENTF_LEFTUP, 0, 0));
        let events: Vec<HookEvent> = rx.recv_timeout(Duration::from_secs(3)).into_iter().chain(rx.recv_timeout(Duration::from_secs(3))).collect();
        assert!(matches!(events.first(), Some(HookEvent::OpenWheel { mode, fresh: true, .. }) if mode == "convert"), "{events:?}");
        assert_eq!(events.get(1), Some(&HookEvent::ScheduleHide), "{events:?}");
    }

    /// The bug from a real session: with the app slow to handle each mouse move, a mode switch
    /// had to wait behind every queued move. Drives the real hooks with SendInput.
    #[cfg(windows)]
    #[test]
    fn real_hook_switches_while_the_mouse_moves() {
        use std::sync::mpsc::channel;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN,
            MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput, VK_LCONTROL, VK_LSHIFT,
        };
        fn mouse(flags: u32, dx: i32, dy: i32) -> INPUT {
            INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
        }
        fn key(vk: u16, up: bool) -> INPUT {
            let flags = if up { KEYEVENTF_KEYUP } else { 0 };
            INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
        }
        fn send(inputs: &[INPUT]) {
            unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
            std::thread::sleep(Duration::from_millis(60));
        }

        let _g = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = channel();
        // Each move takes the app 5 ms (like a round trip to the main thread)
        start_hook(true, move |ev| {
            if matches!(ev, HookEvent::CursorMove { .. }) {
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = tx.send((ev, Instant::now()));
        });
        std::thread::sleep(Duration::from_millis(300));
        send(&[mouse(MOUSEEVENTF_LEFTDOWN, 0, 0), mouse(MOUSEEVENTF_MOVE, 20, 0)]);
        send(&[key(VK_LSHIFT, false), key(VK_LSHIFT, true)]);
        // 400 moves in one go: 2 s of work if each were handled
        let moves: Vec<INPUT> = (0..400).map(|i| mouse(MOUSEEVENTF_MOVE, if i % 2 == 0 { 1 } else { -1 }, 0)).collect();
        send(&moves);
        let switched_at = Instant::now();
        send(&[key(VK_LCONTROL, false), key(VK_LCONTROL, true)]);
        send(&[mouse(MOUSEEVENTF_LEFTUP, 0, 0)]);

        let mut got = Vec::new();
        while let Ok(e) = rx.recv_timeout(Duration::from_secs(3)) {
            let hide = e.0 == HookEvent::ScheduleHide;
            got.push(e);
            if hide {
                break;
            }
        }
        let names: Vec<String> = got.iter().map(|(e, _)| format!("{e:?}")).collect();
        let tools = got.iter().find(|(e, _)| matches!(e, HookEvent::OpenWheel { mode, fresh: false, .. } if mode == "tools"));
        let (_, at) = tools.unwrap_or_else(|| panic!("no switch to tools: {names:?}"));
        assert!(at.duration_since(switched_at) < Duration::from_millis(400), "switch took {:?}", at.duration_since(switched_at));
        let moves = got.iter().filter(|(e, _)| matches!(e, HookEvent::CursorMove { .. })).count();
        assert!(moves < 100, "{moves} moves were handled one by one");
        assert!(matches!(got.last(), Some((HookEvent::ScheduleHide, _))), "{names:?}");
    }

    #[test]
    fn global_feed_does_not_deadlock_with_reset() {
        // The old hook held its lock while calling back into the app, which then called
        // reset_drag() and deadlocked. Hammer both from several threads.
        let _g = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
        start_hook(true, |_| {});
        let workers: Vec<_> = (0..4)
            .map(|i| {
                std::thread::spawn(move || {
                    for n in 0..2000 {
                        match (i + n) % 4 {
                            0 => {
                                feed(Input::MouseDown { x: 0, y: 0 }, (0, 0));
                            }
                            1 => {
                                feed(Input::MouseMove { x: n, y: 0 }, (n, 0));
                            }
                            2 => reset_drag(),
                            _ => set_shown(n % 2 == 0),
                        }
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
    }
}
