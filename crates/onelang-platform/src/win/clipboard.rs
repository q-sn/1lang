//! Clipboard snapshot / restore and plain-text access.

use std::thread;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

use crate::{Error, Result};

/// Formats whose data is a GDI handle, not an HGLOBAL; they are synthesized
/// by Windows from other formats (e.g. CF_BITMAP from CF_DIB) anyway.
const HANDLE_FORMATS: [u32; 8] = [2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E];
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

struct Guard;

impl Guard {
    fn open() -> Result<Guard> {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            if unsafe { OpenClipboard(Some(HWND::default())) }.is_ok() {
                return Ok(Guard);
            }
            if Instant::now() > deadline {
                return Err(Error::ClipboardBusy);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Everything that was on the clipboard, to put it back after we borrowed it.
#[derive(Default)]
pub struct Snapshot(Vec<(u32, Vec<u8>)>);

impl Snapshot {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

unsafe fn read_hglobal(h: HANDLE) -> Option<Vec<u8>> {
    let hg = HGLOBAL(h.0);
    let size = GlobalSize(hg);
    if size == 0 {
        return None;
    }
    let ptr = GlobalLock(hg) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let data = std::slice::from_raw_parts(ptr, size).to_vec();
    let _ = GlobalUnlock(hg);
    Some(data)
}

unsafe fn set_bytes(format: u32, bytes: &[u8]) -> Result<()> {
    let hg = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
    let ptr = GlobalLock(hg) as *mut u8;
    if ptr.is_null() {
        return Err(Error::Other("GlobalLock failed".into()));
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    let _ = GlobalUnlock(hg);
    // On success the system owns the memory.
    SetClipboardData(format, Some(HANDLE(hg.0)))?;
    Ok(())
}

pub fn snapshot() -> Result<Snapshot> {
    let _g = Guard::open()?;
    let mut items = vec![];
    let mut total = 0usize;
    let mut fmt = 0u32;
    unsafe {
        loop {
            fmt = EnumClipboardFormats(fmt);
            if fmt == 0 {
                break;
            }
            if HANDLE_FORMATS.contains(&fmt) {
                continue;
            }
            let Ok(h) = GetClipboardData(fmt) else { continue };
            if h.is_invalid() {
                continue;
            }
            if let Some(data) = read_hglobal(h) {
                total += data.len();
                if total > MAX_SNAPSHOT_BYTES {
                    log::warn!("clipboard snapshot is too large, not saved");
                    return Ok(Snapshot::default());
                }
                items.push((fmt, data));
            }
        }
    }
    Ok(Snapshot(items))
}

pub fn restore(snapshot: &Snapshot) -> Result<()> {
    let _g = Guard::open()?;
    unsafe {
        EmptyClipboard()?;
        for (fmt, data) in &snapshot.0 {
            if let Err(e) = set_bytes(*fmt, data) {
                log::debug!("restore format {fmt}: {e}");
            }
        }
    }
    Ok(())
}

pub fn read_text() -> Result<Option<String>> {
    let _g = Guard::open()?;
    unsafe {
        let Ok(h) = GetClipboardData(CF_UNICODETEXT.0 as u32) else { return Ok(None) };
        let Some(bytes) = read_hglobal(h) else { return Ok(None) };
        let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        Ok(Some(String::from_utf16_lossy(&wide[..end])))
    }
}

/// Put text on the clipboard. `private` asks Windows clipboard history and
/// clipboard managers not to record it.
pub fn write_text(text: &str, private: bool) -> Result<()> {
    let _g = Guard::open()?;
    unsafe {
        EmptyClipboard()?;
        let wide: Vec<u8> = text
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(|c| c.to_le_bytes())
            .collect();
        set_bytes(CF_UNICODETEXT.0 as u32, &wide)?;
        if private {
            let no_history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
            set_bytes(no_history, &0u32.to_le_bytes())?;
            let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
            set_bytes(exclude, &[0])?;
        }
    }
    Ok(())
}
