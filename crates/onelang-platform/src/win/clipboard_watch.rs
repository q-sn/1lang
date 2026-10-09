//! Clipboard change notifications (AddClipboardFormatListener) on a
//! message-only window. Used to detect "copy twice" without a keyboard hook.

use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetWindowLongPtrW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetWindowLongPtrW, TranslateMessage, GWLP_USERDATA, HWND_MESSAGE, MSG,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE, WM_CLOSE, WM_DESTROY, WNDCLASSW,
};

use crate::Result;

type Callback = Box<dyn Fn() + Send>;

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Callback;
            if !ptr.is_null() {
                (*ptr)();
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = RemoveClipboardFormatListener(hwnd);
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Callback;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct ClipboardWatcher {
    hwnd: isize,
    handle: Option<JoinHandle<()>>,
}

impl ClipboardWatcher {
    pub fn start(on_change: impl Fn() + Send + 'static) -> Result<ClipboardWatcher> {
        let (ready_tx, ready_rx) = mpsc::channel::<std::result::Result<isize, String>>();
        let handle = thread::Builder::new()
            .name("clipboard-watch".into())
            .spawn(move || unsafe {
                let hinst = GetModuleHandleW(None).unwrap_or_default();
                let class = w!("OneLangClipboardWatcher");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wnd_proc),
                    hInstance: hinst.into(),
                    lpszClassName: class,
                    ..Default::default()
                };
                RegisterClassW(&wc);
                let hwnd = match CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!(""),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(hinst.into()),
                    None,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let cb: Box<Callback> = Box::new(Box::new(on_change));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(cb) as isize);
                if let Err(e) = AddClipboardFormatListener(hwnd) {
                    let _ = ready_tx.send(Err(e.to_string()));
                    let _ = DestroyWindow(hwnd);
                    return;
                }
                let _ = ready_tx.send(Ok(hwnd.0 as isize));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .map_err(|e| crate::Error::Other(e.to_string()))?;
        let hwnd = ready_rx
            .recv()
            .map_err(|e| crate::Error::Other(e.to_string()))?
            .map_err(crate::Error::Other)?;
        Ok(ClipboardWatcher { hwnd, handle: Some(handle) })
    }
}

impl Drop for ClipboardWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
