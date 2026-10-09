//! User-facing flows: translate selection / clipboard, replace, OCR, icon.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

use onelang_core::{TranslateMode, TranslateRequest};
use onelang_platform::selection::{self, Options};
use onelang_platform::{capture, clipboard, input, ocr, system, ForegroundApp, Rect, Selection, SelectionMethod};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tauri_specta::Event;

use crate::db::{now_ms, Origin};
use crate::events::{IconBusy, RegionShow};
use crate::service::{self, RunOptions};
use crate::settings::{Anchor, Settings};
use crate::state::{AppExt, PopupPayload};
use crate::windows::{self, Anchoring};

fn is_self(app: &ForegroundApp) -> bool {
    app.pid == std::process::id()
}

pub fn is_excluded(settings: &Settings, app: &ForegroundApp) -> bool {
    is_self(app)
        || settings
            .selection
            .excluded_apps
            .iter()
            .any(|e| e.trim().eq_ignore_ascii_case(&app.exe))
}

fn suppress_clipboard(app: &AppHandle, ms: i64) {
    app.state_ref().suppress_clipboard_until.store(now_ms() + ms, Ordering::Relaxed);
}

fn payload(text: String, context: Option<String>, origin: Origin, fg: Option<&ForegroundApp>) -> PopupPayload {
    PopupPayload {
        id: 0,
        text,
        context,
        origin,
        app: fg.map(|a| a.exe.clone()).filter(|e| !e.is_empty()),
        translate: true,
        can_replace: matches!(origin, Origin::Selection),
        notice: None,
    }
}

/// Ctrl+C in a console without a selection interrupts the running program.
fn is_terminal(fg: Option<&ForegroundApp>) -> bool {
    const TERMINALS: [&str; 12] = [
        "windowsterminal.exe", "conhost.exe", "openconsole.exe", "cmd.exe", "powershell.exe", "pwsh.exe",
        "mintty.exe", "wezterm-gui.exe", "alacritty.exe", "kitty.exe", "conemu64.exe", "tabby.exe",
    ];
    fg.is_some_and(|a| TERMINALS.contains(&a.exe.as_str()))
}

fn read_selection(app: &AppHandle, method: SelectionMethod) -> Option<Selection> {
    let s = app.state_ref().settings();
    let method = if is_terminal(system::foreground_app().as_ref()) { SelectionMethod::Uia } else { method };
    if method != SelectionMethod::Uia {
        suppress_clipboard(app, 1500);
    }
    match selection::get(&Options {
        method,
        with_context: s.translation.use_context,
        restore_clipboard: s.selection.restore_clipboard,
    }) {
        Ok(sel) => sel.filter(|s| !s.text.trim().is_empty()),
        Err(e) => {
            log::warn!("selection: {e}");
            None
        }
    }
}

/// Hotkey: translate the selected text in a popup.
pub fn translate_selection(app: AppHandle) {
    thread::spawn(move || {
        let fg = system::foreground_app();
        if fg.as_ref().is_some_and(is_self) {
            return;
        }
        windows::hide_icon(&app);
        let method = app.state_ref().settings().selection.method;
        let cursor = system::cursor_position();
        match read_selection(&app, method) {
            Some(sel) => windows::open_popup(
                &app,
                payload(sel.text, sel.context, Origin::Selection, fg.as_ref()),
                Anchoring { bounds: sel.bounds, cursor },
            ),
            None => {
                let mut p = payload(String::new(), None, Origin::Selection, fg.as_ref());
                p.translate = false;
                p.can_replace = false;
                p.notice = Some("no_selection".into());
                windows::open_popup(&app, p, Anchoring { bounds: None, cursor });
            }
        }
    });
}

/// Hotkey: replace the selection with its translation, without a popup.
pub fn replace_selection(app: AppHandle) {
    thread::spawn(move || {
        let fg = system::foreground_app();
        if fg.as_ref().is_some_and(is_self) {
            return;
        }
        let settings = app.state_ref().settings();
        let cursor = system::cursor_position();
        let Some(sel) = read_selection(&app, settings.selection.method) else { return };

        {
            // The icon becomes a spinner: no stale selection, no pending auto-hide.
            let state = app.state_ref();
            let mut icon = state.icon.lock().unwrap();
            icon.busy = true;
            icon.selection = None;
            icon.generation += 1;
        }
        windows::show_icon(&app, windows::IconAnchor::Cursor, cursor);
        let _ = IconBusy(true).emit_to(&app, windows::ICON);
        let req = TranslateRequest {
            text: sel.text.clone(),
            mode: TranslateMode::Translate,
            context: sel.context.clone(),
            formality: settings.translation.formality,
            ..Default::default()
        };
        let res = tauri::async_runtime::block_on(service::run(
            &app,
            req,
            RunOptions { origin: Origin::Replace, app: fg.as_ref().map(|a| a.exe.clone()), save_history: true },
            Arc::new(|_| {}),
        ));
        let _ = IconBusy(false).emit_to(&app, windows::ICON);
        app.state_ref().icon.lock().unwrap().busy = false;
        windows::hide_icon(&app);
        match res {
            Ok(r) => {
                suppress_clipboard(&app, 2000);
                if let Err(e) = input::paste_text(&r.text) {
                    log::error!("paste: {e}");
                }
            }
            Err(e) => {
                let mut p = payload(sel.text, sel.context, Origin::Selection, fg.as_ref());
                p.translate = false;
                p.notice = Some(e);
                windows::open_popup(&app, p, Anchoring { bounds: sel.bounds, cursor });
            }
        }
    });
}

/// Ctrl+C+C or hotkey: translate what is on the clipboard.
pub fn translate_clipboard(app: AppHandle, automatic: bool) {
    thread::spawn(move || {
        let fg = system::foreground_app();
        if automatic {
            let s = app.state_ref().settings();
            if s.paused || fg.as_ref().is_some_and(|a| is_excluded(&s, a)) {
                return;
            }
        }
        let text = match clipboard::read_text() {
            Ok(Some(t)) if !t.trim().is_empty() => t,
            _ => return,
        };
        windows::hide_icon(&app);
        let cursor = system::cursor_position();
        windows::open_popup(
            &app,
            payload(text, None, Origin::Clipboard, fg.as_ref()),
            Anchoring { bounds: None, cursor },
        );
    });
}

// ---------- floating icon ----------

/// After a mouse selection: read it (UIA only by default) and show the button.
pub fn check_selection_for_icon(app: &AppHandle) {
    let state = app.state_ref();
    let settings = state.settings();
    if settings.paused || !settings.selection.icon_enabled {
        return;
    }
    let Some(fg) = system::foreground_app() else { return };
    if is_excluded(&settings, &fg) {
        return;
    }
    let method = if settings.selection.icon_clipboard_fallback {
        SelectionMethod::UiaThenClipboard
    } else {
        SelectionMethod::Uia
    };
    let cursor = system::cursor_position();
    let Some(sel) = read_selection(app, method) else {
        windows::hide_icon(app);
        return;
    };
    if sel.text.trim().chars().count() < 2 && !sel.text.trim().chars().any(|c| c.is_alphabetic()) {
        return;
    }
    let generation = {
        let mut icon = state.icon.lock().unwrap();
        if icon.visible && icon.selection.as_ref().is_some_and(|s| s.text == sel.text) {
            return;
        }
        icon.generation += 1;
        icon.selection = Some(sel.clone());
        icon.app = Some(fg.exe.clone());
        icon.generation
    };
    let use_bounds = settings.selection.icon_anchor == Anchor::Selection;
    let anchor = match (use_bounds, sel.start_bounds, sel.end_bounds) {
        (true, Some(start), Some(end)) if !start.is_empty() && !end.is_empty() => {
            windows::IconAnchor::Selection { start, end }
        }
        (true, _, _) => {
            log::debug!("selection bounds unavailable ({}), placing the button at the cursor", fg.exe);
            windows::IconAnchor::Cursor
        }
        _ => windows::IconAnchor::Cursor,
    };
    windows::show_icon(app, anchor, cursor);

    let timeout = settings.selection.icon_timeout_ms;
    if timeout > 0 {
        let app = app.clone();
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(timeout as u64));
            let same = app.state_ref().icon.lock().unwrap().generation == generation;
            if same {
                windows::hide_icon(&app);
            }
        });
    }
}

pub fn icon_clicked(app: &AppHandle) {
    let state = app.state_ref();
    let (sel, exe) = {
        let mut icon = state.icon.lock().unwrap();
        (icon.selection.take(), icon.app.clone())
    };
    windows::hide_icon(app);
    let Some(sel) = sel else { return };
    let fg = exe.map(|exe| ForegroundApp { exe, ..Default::default() });
    let cursor = system::cursor_position();
    windows::open_popup(
        app,
        payload(sel.text, sel.context, Origin::Selection, fg.as_ref()),
        Anchoring { bounds: sel.bounds, cursor },
    );
}

// ---------- OCR ----------

fn region_label(i: u32) -> String {
    format!("region-{i}")
}

pub fn region_payload(app: &AppHandle, index: u32) -> Option<RegionShow> {
    let state = app.state_ref();
    let caps = state.captures.lock().unwrap();
    let shot = caps.get(index as usize)?;
    Some(RegionShow {
        index,
        url: format!("http://capture.localhost/{index}?t={}", now_ms()),
        scale_factor: shot.info.scale_factor,
    })
}

/// OCR button in the main window: the window must not cover the screen.
pub fn start_ocr_from_main(app: AppHandle) {
    app.state_ref().ocr_to_main.store(true, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(windows::MAIN) {
        let _ = w.hide();
    }
    start_ocr(app);
}

pub fn start_ocr(app: AppHandle) {
    if app.state_ref().ocr_active.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        windows::hide_icon(&app);
        if app.state_ref().ocr_to_main.load(Ordering::SeqCst) {
            // Let the window disappear (DWM animation) before the screenshot.
            thread::sleep(std::time::Duration::from_millis(220));
        }
        let shots = match capture::capture_all() {
            Ok(s) => s,
            Err(e) => {
                log::error!("capture: {e}");
                app.state_ref().ocr_active.store(false, Ordering::SeqCst);
                restore_main_after_ocr(&app, None);
                return;
            }
        };
        let infos: Vec<_> = shots.iter().map(|s| s.info.clone()).collect();
        *app.state_ref().captures.lock().unwrap() = shots;

        for info in &infos {
            let label = region_label(info.index);
            let window = match app.get_webview_window(&label) {
                Some(w) => w,
                None => match WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
                    .title("1lang OCR")
                    .decorations(false)
                    .resizable(false)
                    .shadow(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .visible(false)
                    .build()
                {
                    Ok(w) => w,
                    Err(e) => {
                        log::error!("region window: {e}");
                        continue;
                    }
                },
            };
            let _ = window.set_position(PhysicalPosition::new(info.x, info.y));
            let _ = window.set_size(PhysicalSize::new(info.width, info.height));
            if let Some(p) = region_payload(&app, info.index) {
                let _ = p.emit_to(&app, label.as_str());
            }
        }
    });
}

/// The region window has loaded its screenshot and can be shown.
pub fn region_ready(app: &AppHandle, index: u32) {
    let Some(w) = app.get_webview_window(&region_label(index)) else { return };
    let _ = w.show();
    let cursor = system::cursor_position();
    let state = app.state_ref();
    let under_cursor = state.captures.lock().unwrap().get(index as usize).is_some_and(|s| {
        let i = &s.info;
        cursor.x >= i.x && cursor.x < i.x + i.width as i32 && cursor.y >= i.y && cursor.y < i.y + i.height as i32
    });
    if under_cursor {
        let _ = w.set_focus();
        if let Ok(h) = w.hwnd() {
            onelang_platform::window::force_foreground(h.0 as isize);
        }
    }
}

fn hide_regions(app: &AppHandle) {
    app.state_ref().ocr_active.store(false, Ordering::SeqCst);
    for (label, w) in app.webview_windows() {
        if label.starts_with("region-") {
            let _ = w.hide();
        }
    }
}

pub fn region_cancel(app: &AppHandle) {
    hide_regions(app);
    app.state_ref().captures.lock().unwrap().clear();
    restore_main_after_ocr(app, None);
}

/// Returns true when the main window took over (OCR was started from it).
fn restore_main_after_ocr(app: &AppHandle, text: Option<String>) -> bool {
    if !app.state_ref().ocr_to_main.swap(false, Ordering::SeqCst) {
        return false;
    }
    windows::show_main(app);
    if let Some(text) = text {
        let _ = crate::events::MainOpenText { text, translate: true }.emit_to(app, windows::MAIN);
    }
    true
}

/// `x, y, w, h` in physical pixels relative to monitor `index`.
pub fn region_selected(app: AppHandle, index: u32, x: f64, y: f64, w: f64, h: f64) {
    hide_regions(&app);
    let shot = {
        let state = app.state_ref();
        let mut caps = state.captures.lock().unwrap();
        let shot = caps.get(index as usize).map(|s| {
            (
                s.info.clone(),
                capture::crop(&s.image, x.max(0.0) as u32, y.max(0.0) as u32, w.max(1.0) as u32, h.max(1.0) as u32),
            )
        });
        caps.clear();
        shot
    };
    let Some((info, image)) = shot else { return };
    thread::spawn(move || {
        let settings = app.state_ref().settings();
        let bounds = Rect { x: info.x as f64 + x, y: info.y as f64 + y, width: w, height: h };
        let cursor = system::cursor_position();
        let lang = Some(settings.ocr.language.as_str()).filter(|l| *l != "auto");
        let mut p = payload(String::new(), None, Origin::Ocr, None);
        let result = ocr::recognize(&image, lang, &settings.languages.favorites);
        if let Ok(out) = &result {
            if !out.text.trim().is_empty() && restore_main_after_ocr(&app, Some(out.text.clone())) {
                return;
            }
        }
        restore_main_after_ocr(&app, None);
        match result {
            Ok(out) if !out.text.trim().is_empty() => {
                p.text = out.text;
                p.translate = settings.ocr.translate;
            }
            Ok(_) => {
                p.translate = false;
                p.notice = Some("ocr_empty".into());
            }
            Err(e) => {
                p.translate = false;
                p.notice = Some(e.to_string());
            }
        }
        windows::open_popup(&app, p, Anchoring { bounds: Some(bounds), cursor });
    });
}
