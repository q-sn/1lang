//! Misc system information: foreground app, cursor, accent color.

use windows::Win32::Foundation::{CloseHandle, HWND, POINT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};
use windows::UI::ViewManagement::{UIColorType, UISettings};

use crate::{ForegroundApp, Point, Result};

pub fn cursor_position() -> Point {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    Point { x: p.x, y: p.y }
}

pub fn foreground_hwnd() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub fn foreground_app() -> Option<ForegroundApp> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let len = GetWindowTextLengthW(hwnd);
        let mut title = vec![0u16; (len + 1).max(1) as usize];
        let n = GetWindowTextW(hwnd, &mut title);
        let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);

        let mut exe = String::new();
        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = vec![0u16; 1024];
            let mut size = buf.len() as u32;
            if QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), windows::core::PWSTR(buf.as_mut_ptr()), &mut size)
                .is_ok()
            {
                let path = String::from_utf16_lossy(&buf[..size as usize]);
                exe = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_lowercase();
            }
            let _ = CloseHandle(h);
        }
        Some(ForegroundApp { exe, title, pid })
    }
}

/// System accent colors as `#rrggbb`: [accent, light1, light2, dark1, dark2].
pub fn accent_colors() -> Result<Vec<String>> {
    let s = UISettings::new()?;
    let mut out = vec![];
    for t in [
        UIColorType::Accent,
        UIColorType::AccentLight1,
        UIColorType::AccentLight2,
        UIColorType::AccentDark1,
        UIColorType::AccentDark2,
    ] {
        let c = s.GetColorValue(t)?;
        out.push(format!("#{:02x}{:02x}{:02x}", c.R, c.G, c.B));
    }
    Ok(out)
}
