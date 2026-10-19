//! Low-level keyboard and mouse hooks (WH_KEYBOARD_LL / WH_MOUSE_LL).
//!
//! The hook procedures only forward events into a channel; all logic runs on
//! the consumer side, because Windows drops hooks that are slow to return.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, VK_CONTROL};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KillTimer, PostThreadMessageW, SetTimer, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, HOOKPROC, WM_TIMER, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::{Point, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy)]
pub enum HookEvent {
    /// `ctrl`: Ctrl was held at that moment; `time`: event time in ms (GetTickCount).
    KeyDown { vk: u32, ctrl: bool, time: u32 },
    KeyUp { vk: u32 },
    MouseDown { button: MouseButton, pos: Point },
    MouseUp { button: MouseButton, pos: Point },
}

static SENDER: Mutex<Option<Sender<HookEvent>>> = Mutex::new(None);
/// Event time (GetTickCount) of the last call into any of our hooks.
static LAST_SEEN: AtomicU32 = AtomicU32::new(0);

fn forward(ev: HookEvent) {
    if let Ok(guard) = SENDER.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(ev);
        }
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        LAST_SEEN.store(info.time, Ordering::Relaxed);
        if (info.flags.0 & LLKHF_INJECTED.0) == 0 {
            match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    // Read here, not by the consumer: by then Ctrl may be released already.
                    let ctrl = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
                    forward(HookEvent::KeyDown { vk: info.vkCode, ctrl, time: info.time })
                }
                WM_KEYUP | WM_SYSKEYUP => forward(HookEvent::KeyUp { vk: info.vkCode }),
                _ => {}
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        LAST_SEEN.store(info.time, Ordering::Relaxed);
        if (info.flags & LLMHF_INJECTED) == 0 {
            let pos = Point { x: info.pt.x, y: info.pt.y };
            let ev = match wparam.0 as u32 {
                WM_LBUTTONDOWN => Some(HookEvent::MouseDown { button: MouseButton::Left, pos }),
                WM_RBUTTONDOWN => Some(HookEvent::MouseDown { button: MouseButton::Right, pos }),
                WM_MBUTTONDOWN => Some(HookEvent::MouseDown { button: MouseButton::Middle, pos }),
                WM_LBUTTONUP => Some(HookEvent::MouseUp { button: MouseButton::Left, pos }),
                _ => None,
            };
            if let Some(ev) = ev {
                forward(ev);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Running hooks; dropping it removes them.
pub struct Hooks {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

impl Hooks {
    pub fn start(keyboard: bool, mouse: bool) -> Result<(Hooks, Receiver<HookEvent>)> {
        let (tx, rx) = mpsc::channel();
        *SENDER.lock().unwrap() = Some(tx);
        let (ready_tx, ready_rx) = mpsc::channel::<std::result::Result<u32, String>>();
        let handle = thread::Builder::new()
            .name("input-hooks".into())
            .spawn(move || unsafe {
                let mut hooks = match install(keyboard, mouse) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                mark_seen_now();
                let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                // Thread timer: WM_TIMER arrives through GetMessage with no window.
                let timer = SetTimer(None, 0, WATCHDOG_MS, None);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_TIMER && msg.hwnd.0.is_null() && msg.wParam.0 == timer {
                        if !seem_alive(WATCHDOG_MS + 1000) {
                            // New hooks first, then the old ones: both are called only from
                            // this thread's loop, so no event is lost or seen twice.
                            match install(keyboard, mouse) {
                                Ok(new) => {
                                    log::warn!("input hooks got no events, reinstalled");
                                    for h in std::mem::replace(&mut hooks, new) {
                                        let _ = UnhookWindowsHookEx(h);
                                    }
                                }
                                Err(e) => log::error!("reinstalling input hooks: {e}"),
                            }
                            mark_seen_now();
                        }
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = KillTimer(None, timer);
                for h in hooks {
                    let _ = UnhookWindowsHookEx(h);
                }
            })
            .map_err(|e| crate::Error::Other(e.to_string()))?;
        let thread_id = ready_rx
            .recv()
            .map_err(|e| crate::Error::Other(e.to_string()))?
            .map_err(crate::Error::Other)?;
        Ok((Hooks { thread_id, handle: Some(handle) }, rx))
    }
}

const WATCHDOG_MS: u32 = 2000;

unsafe fn install(keyboard: bool, mouse: bool) -> std::result::Result<Vec<HHOOK>, String> {
    let hinst = GetModuleHandleW(None).ok().map(|m| m.into());
    let mut hooks: Vec<HHOOK> = vec![];
    let mut add = |kind, proc_: HOOKPROC| match SetWindowsHookExW(kind, proc_, hinst, 0) {
        Ok(h) => {
            hooks.push(h);
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    };
    let res = (|| {
        if keyboard {
            add(WH_KEYBOARD_LL, Some(keyboard_proc))?;
        }
        if mouse {
            add(WH_MOUSE_LL, Some(mouse_proc))?;
        }
        Ok(())
    })();
    match res {
        Ok(()) => Ok(hooks),
        Err(e) => {
            for h in hooks {
                let _ = UnhookWindowsHookEx(h);
            }
            Err(e)
        }
    }
}

fn mark_seen_now() {
    if let Some(t) = last_input_time() {
        LAST_SEEN.store(t, Ordering::Relaxed);
    }
}

/// Windows silently removes a low-level hook whose procedure was too slow to
/// return (it happens under load, e.g. right after logon). Returns false when
/// the system got input that none of our hooks saw for `grace_ms`. Some input
/// (e.g. zero-distance mouse moves) never reaches hooks, so a false alarm is
/// possible and must be cheap.
fn seem_alive(grace_ms: u32) -> bool {
    let Some(input) = last_input_time() else { return true };
    let seen = LAST_SEEN.load(Ordering::Relaxed);
    (input.wrapping_sub(seen) as i32) < grace_ms as i32
}

fn last_input_time() -> Option<u32> {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe { GetLastInputInfo(&mut info) }.as_bool().then_some(info.dwTime)
}

impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        *SENDER.lock().unwrap() = None;
    }
}

pub mod vk {
    pub const C: u32 = 0x43;
    pub const A: u32 = 0x41;
    pub const SHIFT: u32 = 0x10;
    pub const LSHIFT: u32 = 0xA0;
    pub const RSHIFT: u32 = 0xA1;
    pub const CONTROL: u32 = 0x11;
    pub const LCONTROL: u32 = 0xA2;
    pub const RCONTROL: u32 = 0xA3;
    pub const ESCAPE: u32 = 0x1B;
    pub const LEFT: u32 = 0x25;
    pub const UP: u32 = 0x26;
    pub const RIGHT: u32 = 0x27;
    pub const DOWN: u32 = 0x28;
    pub const HOME: u32 = 0x24;
    pub const END: u32 = 0x23;

    pub fn is_ctrl(vk: u32) -> bool {
        matches!(vk, CONTROL | LCONTROL | RCONTROL)
    }
    pub fn is_shift(vk: u32) -> bool {
        matches!(vk, SHIFT | LSHIFT | RSHIFT)
    }
}
