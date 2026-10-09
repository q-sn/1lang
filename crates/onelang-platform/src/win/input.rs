//! Synthetic keyboard input: copy / paste on behalf of the user.

use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};

use super::clipboard;
use crate::Result;

const MODIFIERS: [VIRTUAL_KEY; 9] =
    [VK_CONTROL, VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_MENU, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN];

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
}

/// Wait until the user releases the hotkey modifiers, otherwise our Ctrl+C
/// would become e.g. Ctrl+Alt+C.
pub fn wait_modifiers_released(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while MODIFIERS.iter().any(|&m| is_down(m)) {
        if Instant::now() > deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
    true
}

fn send_ctrl(letter: VIRTUAL_KEY) {
    let inputs = [key(VK_CONTROL, false), key(letter, false), key(letter, true), key(VK_CONTROL, true)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Simulate Ctrl+C and return what got copied. The previous clipboard content
/// is restored when `restore` is set.
pub fn copy_selection(restore: bool) -> Result<Option<String>> {
    wait_modifiers_released(Duration::from_millis(800));
    let saved = if restore { Some(clipboard::snapshot()?) } else { None };
    let before = clipboard::sequence_number();
    send_ctrl(VK_C);

    let deadline = Instant::now() + Duration::from_millis(450);
    let mut changed = false;
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
        if clipboard::sequence_number() != before {
            changed = true;
            // Some apps write several formats one after another.
            thread::sleep(Duration::from_millis(25));
            break;
        }
    }
    let text = if changed { clipboard::read_text()? } else { None };
    if let Some(saved) = saved {
        if changed {
            if let Err(e) = clipboard::restore(&saved) {
                log::warn!("failed to restore clipboard: {e}");
            }
        }
    }
    Ok(text.filter(|t| !t.trim().is_empty()))
}

/// Replace the current selection in the foreground app with `text` (via paste).
pub fn paste_text(text: &str) -> Result<()> {
    wait_modifiers_released(Duration::from_millis(800));
    let saved = clipboard::snapshot().ok();
    clipboard::write_text(text, true)?;
    send_ctrl(VK_V);
    // Give the target app time to read the clipboard before restoring
    // (RDP, Java and some Electron apps read it late).
    thread::sleep(Duration::from_millis(650));
    if let Some(saved) = saved {
        let _ = clipboard::restore(&saved);
    }
    Ok(())
}

pub fn is_ctrl_down() -> bool {
    is_down(VK_CONTROL)
}
