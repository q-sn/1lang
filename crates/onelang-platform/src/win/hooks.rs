//! Low-level keyboard and mouse hooks (WH_KEYBOARD_LL / WH_MOUSE_LL).
//!
//! The hook procedures only forward events into a channel; all logic runs on
//! the consumer side, because Windows drops hooks that are slow to return.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL,
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
    KeyDown { vk: u32 },
    KeyUp { vk: u32 },
    MouseDown { button: MouseButton, pos: Point },
    MouseUp { button: MouseButton, pos: Point },
}

static SENDER: Mutex<Option<Sender<HookEvent>>> = Mutex::new(None);

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
        if (info.flags.0 & LLKHF_INJECTED.0) == 0 {
            match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => forward(HookEvent::KeyDown { vk: info.vkCode }),
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
                let hmod = GetModuleHandleW(None).ok();
                let hinst = hmod.map(|m| m.into());
                let mut hooks: Vec<HHOOK> = vec![];
                if keyboard {
                    match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0) {
                        Ok(h) => hooks.push(h),
                        Err(e) => {
                            let _ = ready_tx.send(Err(e.to_string()));
                            return;
                        }
                    }
                }
                if mouse {
                    match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0) {
                        Ok(h) => hooks.push(h),
                        Err(e) => {
                            for h in hooks {
                                let _ = UnhookWindowsHookEx(h);
                            }
                            let _ = ready_tx.send(Err(e.to_string()));
                            return;
                        }
                    }
                }
                let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
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
