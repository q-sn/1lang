//! Window tweaks Tauri does not expose.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWMWCP_ROUNDSMALL,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut _)
}

/// Rounded corners for undecorated windows on Windows 11 (ignored on Windows 10).
pub fn round_corners(raw: isize, small: bool) {
    let pref: DWM_WINDOW_CORNER_PREFERENCE = if small { DWMWCP_ROUNDSMALL } else { DWMWCP_ROUND };
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd(raw),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

/// A window that never takes focus when clicked (the floating icon), so the
/// selection in the original app stays intact.
pub fn make_non_activating(raw: isize) {
    unsafe {
        let h = hwnd(raw);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize);
    }
}

pub fn focus(raw: isize) -> bool {
    unsafe { SetForegroundWindow(hwnd(raw)).as_bool() }
}

/// Bring a window to the foreground even when Windows' focus-stealing
/// prevention would block it (e.g. popup opened by Ctrl+C+C, not by our own input).
pub fn force_foreground(raw: isize) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, ShowWindow, SW_SHOW,
    };
    unsafe {
        let target = hwnd(raw);
        let fg = GetForegroundWindow();
        if fg == target {
            return;
        }
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = ShowWindow(target, SW_SHOW);
        let _ = BringWindowToTop(target);
        let _ = SetForegroundWindow(target);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
    }
}

/// Show a window without activating it (keeps focus and selection in the current app).
pub fn show_no_activate(raw: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    unsafe {
        let _ = ShowWindow(hwnd(raw), SW_SHOWNOACTIVATE);
    }
}

/// Hide a window shown with [`show_no_activate`] (Tauri's own `hide()` would
/// skip it because its cached visibility state says the window is hidden).
pub fn hide(raw: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    unsafe {
        let _ = ShowWindow(hwnd(raw), SW_HIDE);
    }
}
