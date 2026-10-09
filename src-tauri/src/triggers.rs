//! Global hotkeys, input hooks and the clipboard watcher. Everything is
//! (re)configured from settings by `apply`.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use onelang_platform::clipboard_watch::ClipboardWatcher;
use onelang_platform::hooks::{vk, HookEvent, Hooks, MouseButton};
use onelang_platform::{input, Point};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::actions;
use crate::db::now_ms;
use crate::settings::{DoubleCopyDetection, Settings};
use crate::state::AppExt;
use crate::windows;

type Action = fn(AppHandle);

pub struct TriggerRuntime {
    _hooks: Option<Hooks>,
    _watcher: Option<ClipboardWatcher>,
}

/// Register hotkeys; returns human-readable problems (e.g. a hotkey already taken).
pub fn register_hotkeys(app: &AppHandle, s: &Settings) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut problems = vec![];
    let bindings: [(&str, Action); 5] = [
        (&s.hotkeys.translate_selection, actions::translate_selection),
        (&s.hotkeys.replace_selection, actions::replace_selection),
        (&s.hotkeys.ocr_region, actions::start_ocr),
        (&s.hotkeys.translate_clipboard, |app| actions::translate_clipboard(app, false)),
        (&s.hotkeys.open_main, |app| windows::show_main(&app)),
    ];
    for (accel, action) in bindings {
        let accel = accel.trim();
        if accel.is_empty() {
            continue;
        }
        let res = gs.on_shortcut(accel, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                action(app.clone());
            }
        });
        if let Err(e) = res {
            log::warn!("hotkey {accel}: {e}");
            problems.push(format!("{accel}: {e}"));
        }
    }
    problems
}

/// Start/stop hooks and the clipboard watcher according to settings.
pub fn apply(app: &AppHandle) {
    let state = app.state_ref();
    let s = state.settings();
    // Stop the old runtime first (drops hooks / watcher).
    *state.triggers.lock().unwrap() = None;
    if s.paused {
        windows::hide_icon(app);
        return;
    }

    let keyboard_double_copy =
        s.selection.double_copy_enabled && s.selection.double_copy_detection == DoubleCopyDetection::Keyboard;
    let need_keyboard = s.selection.icon_enabled || keyboard_double_copy;
    let need_mouse = s.selection.icon_enabled;

    let hooks = if need_keyboard || need_mouse {
        match Hooks::start(need_keyboard, need_mouse) {
            Ok((hooks, rx)) => {
                spawn_hook_consumer(app.clone(), rx, keyboard_double_copy, s.selection.double_copy_interval_ms);
                Some(hooks)
            }
            Err(e) => {
                log::error!("hooks: {e}");
                None
            }
        }
    } else {
        None
    };

    let watcher = if s.selection.double_copy_enabled && s.selection.double_copy_detection == DoubleCopyDetection::Clipboard
    {
        let handle = app.clone();
        let interval = s.selection.double_copy_interval_ms as i64;
        let last = std::sync::atomic::AtomicI64::new(0);
        match ClipboardWatcher::start(move || {
            let now = now_ms();
            if now < handle.state_ref().suppress_clipboard_until.load(Ordering::Relaxed) {
                return;
            }
            // Ctrl+C+C means Ctrl is still held on the second copy. This also
            // filters out apps that update the clipboard twice on one copy.
            if !input::is_ctrl_down() {
                last.store(0, Ordering::Relaxed);
                return;
            }
            let prev = last.load(Ordering::Relaxed);
            if prev != 0 && (40..=interval).contains(&(now - prev)) {
                last.store(0, Ordering::Relaxed);
                actions::translate_clipboard(handle.clone(), true);
            } else {
                last.store(now, Ordering::Relaxed);
            }
        }) {
            Ok(w) => Some(w),
            Err(e) => {
                log::error!("clipboard watcher: {e}");
                None
            }
        }
    } else {
        None
    };

    *state.triggers.lock().unwrap() = Some(TriggerRuntime { _hooks: hooks, _watcher: watcher });
}

fn dist(a: Point, b: Point) -> f64 {
    (((a.x - b.x) as f64).powi(2) + ((a.y - b.y) as f64).powi(2)).sqrt()
}

fn spawn_hook_consumer(app: AppHandle, rx: Receiver<HookEvent>, keyboard_double_copy: bool, interval_ms: u32) {
    // Selection checks run on their own thread so the consumer never blocks.
    let (check_tx, check_rx) = mpsc::channel::<Duration>();
    {
        let app = app.clone();
        thread::Builder::new()
            .name("selection-check".into())
            .spawn(move || {
                while let Ok(mut delay) = check_rx.recv() {
                    // Coalesce bursts (e.g. Shift+Arrow held down).
                    loop {
                        thread::sleep(delay);
                        match check_rx.try_recv() {
                            Ok(d) => delay = d,
                            Err(_) => break,
                        }
                    }
                    actions::check_selection_for_icon(&app);
                }
            })
            .ok();
    }

    thread::Builder::new()
        .name("hook-consumer".into())
        .spawn(move || {
            let interval = Duration::from_millis(interval_ms as u64);
            let mut down_pos = Point::default();
            let mut down_at = Instant::now();
            let mut last_up_pos = Point::default();
            let mut last_up_at = Instant::now() - Duration::from_secs(10);
            let mut ctrl = false;
            let mut shift = false;
            let mut last_ctrl_c: Option<Instant> = None;
            // Auto-repeat of a held C must not count as a second press.
            let mut c_down = false;

            while let Ok(ev) = rx.recv() {
                match ev {
                    HookEvent::MouseDown { button, pos } => {
                        let state = app.state_ref();
                        let (visible, rect, busy) = {
                            let icon = state.icon.lock().unwrap();
                            (icon.visible, icon.rect, icon.busy)
                        };
                        if busy {
                            continue;
                        }
                        if visible {
                            let inside = pos.x as f64 >= rect.x
                                && pos.x as f64 <= rect.x + rect.width
                                && pos.y as f64 >= rect.y
                                && pos.y as f64 <= rect.y + rect.height;
                            if inside && button == MouseButton::Left {
                                let app = app.clone();
                                thread::spawn(move || actions::icon_clicked(&app));
                                continue;
                            }
                            windows::hide_icon(&app);
                        }
                        if button == MouseButton::Left {
                            down_pos = pos;
                            down_at = Instant::now();
                        }
                    }
                    HookEvent::MouseUp { button: MouseButton::Left, pos } => {
                        let now = Instant::now();
                        let dragged = dist(pos, down_pos) >= 8.0 && now.duration_since(down_at) >= Duration::from_millis(80);
                        let double_click =
                            now.duration_since(last_up_at) <= Duration::from_millis(500) && dist(pos, last_up_pos) < 6.0;
                        let shift_click = shift && !dragged;
                        last_up_at = now;
                        last_up_pos = pos;
                        if dragged || double_click || shift_click {
                            let _ = check_tx.send(Duration::from_millis(if double_click { 120 } else { 60 }));
                        }
                    }
                    HookEvent::MouseUp { .. } => {}
                    HookEvent::KeyDown { vk: code } => {
                        if vk::is_ctrl(code) {
                            ctrl = true;
                            continue;
                        }
                        if vk::is_shift(code) {
                            shift = true;
                            continue;
                        }
                        if keyboard_double_copy && ctrl && code == vk::C {
                            if c_down {
                                continue;
                            }
                            c_down = true;
                            let now = Instant::now();
                            match last_ctrl_c {
                                Some(t) if now.duration_since(t) <= interval => {
                                    last_ctrl_c = None;
                                    let app = app.clone();
                                    thread::spawn(move || {
                                        // Let the app finish writing to the clipboard.
                                        thread::sleep(Duration::from_millis(120));
                                        actions::translate_clipboard(app, true);
                                    });
                                }
                                _ => last_ctrl_c = Some(now),
                            }
                            continue;
                        }
                        let selecting = (shift
                            && matches!(code, vk::LEFT | vk::RIGHT | vk::UP | vk::DOWN | vk::HOME | vk::END))
                            || (ctrl && code == vk::A);
                        if selecting {
                            let _ = check_tx.send(Duration::from_millis(350));
                        } else if !(ctrl && code == vk::C) {
                            windows::hide_icon(&app);
                        }
                    }
                    HookEvent::KeyUp { vk: code } => {
                        if code == vk::C {
                            c_down = false;
                        }
                        if vk::is_ctrl(code) {
                            ctrl = false;
                        }
                        if vk::is_shift(code) {
                            shift = false;
                        }
                    }
                }
            }
        })
        .ok();
}
