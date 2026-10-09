//! Reading the selected text of the foreground application.
//!
//! 1. UI Automation `TextPattern.GetSelection()` — fast, does not touch the clipboard,
//!    gives the on-screen bounds and lets us read the surrounding paragraphs.
//! 2. Fallback: simulate Ctrl+C and restore the clipboard.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetUBound, SafeArrayUnaccessData};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationTextRange,
    IUIAutomationTreeWalker, TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Paragraph,
    UIA_TextPatternId,
};

use super::input;
use crate::{Rect, Result, Selection, SelectionMethod, SelectionSource};

const UIA_TIMEOUT: Duration = Duration::from_millis(700);
const MAX_PARENTS: usize = 8;
const MAX_CONTEXT_CHARS: usize = 1500;

pub struct Options {
    pub method: SelectionMethod,
    pub with_context: bool,
    /// Restore the clipboard after the Ctrl+C fallback.
    pub restore_clipboard: bool,
}

pub fn get(opts: &Options) -> Result<Option<Selection>> {
    if opts.method != SelectionMethod::Clipboard {
        match uia_with_timeout(opts.with_context) {
            Ok(Some(sel)) => return Ok(Some(sel)),
            Ok(None) => {}
            Err(e) => log::debug!("UIA selection failed: {e}"),
        }
        if opts.method == SelectionMethod::Uia {
            return Ok(None);
        }
    }
    Ok(input::copy_selection(opts.restore_clipboard)?.map(|text| Selection {
        text,
        bounds: None,
        start_bounds: None,
        end_bounds: None,
        context: None,
        source: SelectionSource::Clipboard,
    }))
}

/// UIA can hang on unresponsive apps, so it runs on its own thread with a timeout.
pub fn uia_with_timeout(with_context: bool) -> Result<Option<Selection>> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("uia-selection".into())
        .spawn(move || {
            let _ = tx.send(unsafe { uia_selection(with_context) });
        })
        .map_err(|e| crate::Error::Other(e.to_string()))?;
    rx.recv_timeout(UIA_TIMEOUT).map_err(|_| crate::Error::Timeout)?
}

struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

unsafe fn uia_selection(with_context: bool) -> Result<Option<Selection>> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED).ok();
    let _guard = ComGuard;
    let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
    let focused = automation.GetFocusedElement()?;
    let walker: IUIAutomationTreeWalker = automation.ControlViewWalker()?;

    let mut current: Option<IUIAutomationElement> = Some(focused);
    for _ in 0..=MAX_PARENTS {
        let Some(el) = current.take() else { break };
        if let Some(sel) = selection_of(&el, with_context) {
            return Ok(Some(sel));
        }
        current = walker.GetParentElement(&el).ok();
    }
    Ok(None)
}

unsafe fn selection_of(el: &IUIAutomationElement, with_context: bool) -> Option<Selection> {
    let pattern: IUIAutomationTextPattern = el.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
    let ranges = pattern.GetSelection().ok()?;
    let len = ranges.Length().ok()?;
    let mut text = String::new();
    let mut rects: Vec<Rect> = vec![];
    let mut first: Option<IUIAutomationTextRange> = None;
    for i in 0..len {
        let Ok(range) = ranges.GetElement(i) else { continue };
        if let Ok(t) = range.GetText(-1) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&t.to_string());
        }
        rects.extend(range_rects(&range));
        if first.is_none() {
            first = Some(range);
        }
    }
    if text.trim().is_empty() {
        return None;
    }
    let context = if with_context { first.and_then(|r| context_of(&r, &text)) } else { None };
    let bounds = rects.iter().copied().reduce(Rect::union);
    // The end of the selection: the lowest line, its rightmost piece.
    let end_bounds = rects.iter().copied().reduce(|a, b| {
        let (ab, bb) = (a.y + a.height, b.y + b.height);
        if (bb - ab).abs() < a.height.min(b.height) / 2.0 {
            if b.x + b.width > a.x + a.width { b } else { a }
        } else if bb > ab {
            b
        } else {
            a
        }
    });
    // The start: the highest line, its leftmost piece.
    let start_bounds = rects.iter().copied().reduce(|a, b| {
        if (b.y - a.y).abs() < a.height.min(b.height) / 2.0 {
            if b.x < a.x { b } else { a }
        } else if b.y < a.y {
            b
        } else {
            a
        }
    });
    Some(Selection { text, bounds, start_bounds, end_bounds, context, source: SelectionSource::Uia })
}

/// One paragraph before and after the selection.
unsafe fn context_of(range: &IUIAutomationTextRange, selected: &str) -> Option<String> {
    let ctx = range.Clone().ok()?;
    let _ = ctx.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Paragraph, -1);
    let _ = ctx.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Paragraph, 1);
    let text = ctx.GetText((MAX_CONTEXT_CHARS * 3) as i32).ok()?.to_string();
    let text = text.trim();
    if text.is_empty() || text == selected.trim() {
        return None;
    }
    Some(text.chars().take(MAX_CONTEXT_CHARS * 3).collect())
}

/// One rectangle per line of the range, in physical screen pixels.
unsafe fn range_rects(range: &IUIAutomationTextRange) -> Vec<Rect> {
    let mut result = vec![];
    let Ok(sa) = range.GetBoundingRectangles() else { return result };
    if sa.is_null() {
        return result;
    }
    if let Ok(ub) = SafeArrayGetUBound(sa, 1) {
        let count = (ub + 1) as usize;
        let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
        if SafeArrayAccessData(sa, &mut data).is_ok() && !data.is_null() {
            let values = std::slice::from_raw_parts(data as *const f64, count);
            for chunk in values.chunks_exact(4) {
                let r = Rect { x: chunk[0], y: chunk[1], width: chunk[2], height: chunk[3] };
                if !r.is_empty() {
                    result.push(r);
                }
            }
            let _ = SafeArrayUnaccessData(sa);
        }
    }
    let _ = SafeArrayDestroy(sa);
    result
}

