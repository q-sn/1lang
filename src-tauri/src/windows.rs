//! Window creation, glass effects and placement.

use std::sync::atomic::Ordering;

use onelang_platform::{window as pw, Point, Rect};
use tauri::window::{Color, Effect, EffectsBuilder};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Theme, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::db::now_ms;
use tauri_specta::Event;

use crate::events::{PopupHidden, PopupShow};
use crate::settings::{Settings, ThemeMode, WindowEffect};
use crate::state::{AppExt, PopupPayload};

pub const MAIN: &str = "main";
pub const POPUP: &str = "popup";
pub const ICON: &str = "icon";
pub const ICON_SIZE: f64 = 52.0;

pub fn is_windows_11() -> bool {
    #[cfg(windows)]
    {
        windows_version::OsVersion::current().build >= 22000
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Glass {
    Mica,
    Acrylic,
}

fn glass_for(label: &str, settings: &Settings, win11: bool) -> Option<Glass> {
    // The popup sits on top of arbitrary content: a see-through background
    // makes it hard to read, so it is always solid.
    if label != MAIN {
        return None;
    }
    match settings.general.window_effect {
        WindowEffect::None => None,
        WindowEffect::Auto if !win11 => None,
        WindowEffect::Auto => Some(Glass::Mica),
        WindowEffect::Mica if win11 => Some(Glass::Mica),
        WindowEffect::Mica => None,
        WindowEffect::Acrylic => Some(Glass::Acrylic),
    }
}

/// Whether the window `label` currently has a native glass effect.
pub fn has_glass(app: &AppHandle, label: &str) -> bool {
    let state = app.state_ref();
    let s = state.settings.read().unwrap();
    glass_for(label, &s, state.win11).is_some()
}

fn theme_of(settings: &Settings) -> Option<Theme> {
    match settings.general.theme {
        ThemeMode::System => None,
        ThemeMode::Light => Some(Theme::Light),
        ThemeMode::Dark => Some(Theme::Dark),
    }
}

fn dark_now(window: &WebviewWindow, settings: &Settings) -> bool {
    match settings.general.theme {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => window.theme().map(|t| t == Theme::Dark).unwrap_or(false),
    }
}

pub fn apply_effects(window: &WebviewWindow, settings: &Settings, win11: bool) {
    let _ = window.set_theme(theme_of(settings));
    let dark = dark_now(window, settings);
    let effects = glass_for(window.label(), settings, win11).map(|g| match g {
        Glass::Mica => EffectsBuilder::new()
            .effect(match settings.general.theme {
                ThemeMode::System => Effect::Mica,
                ThemeMode::Light => Effect::MicaLight,
                ThemeMode::Dark => Effect::MicaDark,
            })
            .build(),
        Glass::Acrylic => EffectsBuilder::new()
            .effect(Effect::Acrylic)
            .color(if dark { Color(28, 28, 32, 150) } else { Color(248, 248, 250, 150) })
            .build(),
    });
    if let Err(e) = window.set_effects(effects) {
        log::warn!("set_effects({}): {e}", window.label());
    }
}

pub fn apply_effects_all(app: &AppHandle) {
    let state = app.state_ref();
    let s = state.settings();
    for label in [MAIN, POPUP] {
        if let Some(w) = app.get_webview_window(label) {
            apply_effects(&w, &s, state.win11);
        }
    }
}

fn hwnd(w: &WebviewWindow) -> isize {
    w.hwnd().map(|h| h.0 as isize).unwrap_or(0)
}

pub fn ensure_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(MAIN) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("1lang")
        .inner_size(1100.0, 720.0)
        .min_inner_size(820.0, 540.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .visible(false)
        .center()
        .build()?;
    let state = app.state_ref();
    apply_effects(&w, &state.settings(), state.win11);
    let handle = app.clone();
    w.on_window_event(move |e| {
        if let WindowEvent::CloseRequested { api, .. } = e {
            api.prevent_close();
            if handle.state_ref().settings.read().unwrap().general.close_to_tray {
                if let Some(w) = handle.get_webview_window(MAIN) {
                    let _ = w.hide();
                }
            } else {
                handle.exit(0);
            }
        }
    });
    Ok(w)
}

pub fn show_main(app: &AppHandle) {
    match ensure_main(app) {
        Ok(w) => {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            pw::force_foreground(hwnd(&w));
        }
        Err(e) => log::error!("main window: {e}"),
    }
}

pub fn ensure_popup(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(POPUP) {
        return Ok(w);
    }
    let s = app.state_ref().settings();
    let w = WebviewWindowBuilder::new(app, POPUP, WebviewUrl::App("index.html".into()))
        .title("1lang")
        .inner_size(s.popup.width as f64, s.popup.height as f64)
        .min_inner_size(420.0, 190.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;
    let state = app.state_ref();
    apply_effects(&w, &s, state.win11);
    pw::round_corners(hwnd(&w), false);
    let handle = app.clone();
    w.on_window_event(move |e| match e {
        WindowEvent::Focused(false) => {
            // Focus moving between the window and its webview (a click inside
            // the popup) also reports "unfocused": decide a moment later, by
            // which app really is in the foreground.
            let handle = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(120));
                let ours = onelang_platform::system::foreground_app().is_some_and(|a| a.pid == std::process::id());
                if ours {
                    return;
                }
                let st = handle.state_ref();
                let close = st.settings.read().unwrap().popup.close_on_blur;
                let should_hide = {
                    let p = st.popup.lock().unwrap();
                    p.visible && !p.pinned && close && now_ms() - p.shown_at > 400
                };
                if should_hide {
                    hide_popup(&handle);
                }
            });
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide_popup(&handle);
        }
        _ => {}
    });
    Ok(w)
}

pub fn ensure_icon(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(ICON) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, ICON, WebviewUrl::App("index.html".into()))
        .title("1lang")
        .inner_size(ICON_SIZE, ICON_SIZE)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;
    pw::make_non_activating(hwnd(&w));
    Ok(w)
}

/// Physical work area of the monitor containing `p` (falls back to the primary monitor).
fn work_area(app: &AppHandle, p: Point) -> (Rect, f64) {
    let monitor = app
        .monitor_from_point(p.x as f64, p.y as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    match monitor {
        Some(m) => {
            let wa = m.work_area();
            (
                Rect {
                    x: wa.position.x as f64,
                    y: wa.position.y as f64,
                    width: wa.size.width as f64,
                    height: wa.size.height as f64,
                },
                m.scale_factor(),
            )
        }
        None => (Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 }, 1.0),
    }
}

/// Place a `w`×`h` box next to `anchor` inside the work area: below it if it
/// fits, otherwise above, always fully visible.
fn place(anchor: Rect, w: f64, h: f64, gap: f64, area: Rect) -> (f64, f64) {
    let mut x = anchor.x;
    let below = anchor.y + anchor.height + gap;
    let above = anchor.y - gap - h;
    let mut y = if below + h <= area.y + area.height {
        below
    } else if above >= area.y {
        above
    } else {
        area.y + area.height - h
    };
    x = x.clamp(area.x, (area.x + area.width - w).max(area.x));
    y = y.clamp(area.y, (area.y + area.height - h).max(area.y));
    (x, y)
}

/// Popup width (CSS px at 100% zoom) for a text of `chars` characters:
/// short phrases get a compact window. Same rule as `popupWidth` in the frontend.
fn popup_width(chars: usize, max: u32) -> f64 {
    let w = if chars <= 60 {
        440
    } else if chars <= 160 {
        520
    } else {
        max
    };
    w.min(max.max(420)).max(420) as f64
}

pub struct Anchoring {
    pub bounds: Option<Rect>,
    pub cursor: Point,
}

pub fn open_popup(app: &AppHandle, payload: PopupPayload, anchoring: Anchoring) {
    // Window placement must run on the event loop thread, otherwise the
    // queued set_size / set_position race with show() and the popup flashes
    // at its old place.
    let source_hwnd = onelang_platform::system::foreground_hwnd();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || open_popup_now(&handle, payload, anchoring, source_hwnd));
}

fn open_popup_now(app: &AppHandle, mut payload: PopupPayload, anchoring: Anchoring, source_hwnd: isize) {
    let state = app.state_ref();
    let settings = state.settings();
    let Ok(w) = ensure_popup(app) else { return };

    payload.id = state.popup_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let (was_visible, pinned) = {
        let mut p = state.popup.lock().unwrap();
        let was = p.visible;
        if !was {
            p.pinned = settings.popup.always_pinned;
        }
        // The app the text came from (not our own popup when it is already open).
        if source_hwnd != 0 && source_hwnd != hwnd(&w) {
            p.source_hwnd = source_hwnd;
        }
        p.pending = Some(payload.clone());
        p.visible = true;
        p.shown_at = now_ms();
        (was, p.pinned)
    };

    // A pinned popup stays where the user put it.
    if !(was_visible && pinned) {
        let center = anchoring.bounds.map(|b| Point { x: b.x as i32, y: b.y as i32 }).unwrap_or(anchoring.cursor);
        let (area, sf) = work_area(app, center);
        let zoom = settings.general.ui_scale.max(50) as f64 / 100.0;
        let pw_ = popup_width(payload.text.chars().count(), settings.popup.width) * sf * zoom;
        let ph = settings.popup.height as f64 * sf * zoom;
        let cursor_anchor = Rect {
            x: anchoring.cursor.x as f64 - 24.0 * sf,
            y: anchoring.cursor.y as f64,
            width: 1.0,
            height: 14.0 * sf,
        };
        use crate::settings::PopupPlacement as P;
        let (x, y) = match settings.popup.placement {
            P::Selection => place(anchoring.bounds.unwrap_or(cursor_anchor), pw_, ph, 8.0 * sf, area),
            P::Cursor => place(cursor_anchor, pw_, ph, 8.0 * sf, area),
            P::LastPosition => match (settings.popup.last_x, settings.popup.last_y) {
                (Some(x), Some(y)) => {
                    // Stored in physical pixels: logical ones differ between monitors.
                    let (x, y) = (x as f64, y as f64);
                    let (area, _) = work_area(app, Point { x: x as i32 + 10, y: y as i32 + 10 });
                    place(Rect { x, y, width: 0.0, height: 0.0 }, pw_, ph, 0.0, area)
                }
                _ => place(cursor_anchor, pw_, ph, 8.0 * sf, area),
            },
            P::ScreenCenter => {
                (area.x + (area.width - pw_) / 2.0, area.y + (area.height - ph) / 2.5)
            }
        };
        let _ = w.set_size(PhysicalSize::new(pw_.round() as u32, ph.round() as u32));
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }

    let _ = PopupShow(payload).emit_to(app, POPUP);
    let _ = w.show();
    let _ = w.set_focus();
    pw::force_foreground(hwnd(&w));
}

pub fn hide_popup(app: &AppHandle) {
    let state = app.state_ref();
    let was_pinned;
    {
        let mut p = state.popup.lock().unwrap();
        if !p.visible {
            return;
        }
        p.visible = false;
        was_pinned = p.pinned;
    }
    if let Some(w) = app.get_webview_window(POPUP) {
        // Size follows the content; only the position of a pinned popup is remembered.
        if let (true, Ok(pos)) = (was_pinned, w.outer_position()) {
            let changed = {
                let mut s = state.settings.write().unwrap();
                let before = (s.popup.last_x, s.popup.last_y);
                s.popup.last_x = Some(pos.x);
                s.popup.last_y = Some(pos.y);
                (before != (s.popup.last_x, s.popup.last_y)).then(|| s.clone())
            };
            if let Some(s) = changed {
                let _ = state.settings_file.save(&s);
                crate::commands::broadcast_settings(app, &s, None);
            }
        }
        let _ = w.hide();
        let _ = PopupHidden.emit_to(app, POPUP);
    }
}

/// Where the floating button goes.
pub enum IconAnchor {
    Cursor,
    /// First and last line of the selection: the button sits at whichever
    /// edge of the selected text is closer to the mouse.
    Selection { start: Rect, end: Rect },
}

fn distance(x: f64, y: f64, p: Point) -> f64 {
    ((x - p.x as f64).powi(2) + (y - p.y as f64).powi(2)).sqrt()
}

pub fn show_icon(app: &AppHandle, anchor: IconAnchor, cursor: Point) {
    let Ok(w) = ensure_icon(app) else { return };
    let (area, sf) = work_area(app, cursor);
    let size = ICON_SIZE * sf;
    let gap = 4.0 * sf;
    let (x, y) = match anchor {
        IconAnchor::Selection { start, end } => {
            let end_x = end.x + end.width;
            let to_end = distance(end_x, end.y + end.height / 2.0, cursor);
            let to_start = distance(start.x, start.y + start.height / 2.0, cursor);
            if to_start < to_end {
                // Selected right-to-left: before the first word.
                if start.x - gap - size >= area.x {
                    (start.x - gap - size, start.y + (start.height - size) / 2.0)
                } else {
                    (start.x, start.y - gap - size)
                }
            } else if end_x + gap + size <= area.x + area.width {
                // After the last word.
                (end_x + gap, end.y + (end.height - size) / 2.0)
            } else {
                (end_x - size, end.y + end.height + gap)
            }
        }
        IconAnchor::Cursor => (cursor.x as f64 + 10.0 * sf, cursor.y as f64 + 14.0 * sf),
    };
    let x = x.clamp(area.x, area.x + area.width - size);
    let y = y.clamp(area.y, area.y + area.height - size);
    let _ = w.set_size(PhysicalSize::new(size.round() as u32, size.round() as u32));
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    pw::show_no_activate(hwnd(&w));
    let state = app.state_ref();
    let mut icon = state.icon.lock().unwrap();
    icon.visible = true;
    icon.rect = Rect { x, y, width: size, height: size };
}

pub fn hide_icon(app: &AppHandle) {
    let state = app.state_ref();
    {
        let mut icon = state.icon.lock().unwrap();
        if !icon.visible {
            return;
        }
        icon.visible = false;
        icon.generation += 1;
    }
    if let Some(w) = app.get_webview_window(ICON) {
        pw::hide(hwnd(&w));
    }
}
