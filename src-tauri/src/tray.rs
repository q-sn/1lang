//! Tray icon and its menu (localized with a tiny table; the rest of the UI is in Vue).

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

use crate::actions;
use crate::state::AppExt;
use crate::windows;

const TRAY_ID: &str = "main";

fn ui_lang(app: &AppHandle) -> String {
    let l = app.state_ref().settings().general.ui_language;
    if l == "system" {
        crate::settings::system_language()
    } else {
        l
    }
}

fn tr(lang: &str, key: &str) -> &'static str {
    match (lang, key) {
        ("ru", "open") => "Открыть переводчик",
        ("ru", "ocr") => "Перевести область экрана",
        ("ru", "clipboard") => "Перевести буфер обмена",
        ("ru", "pause") => "Пауза авто-перевода",
        ("ru", "history") => "История",
        ("ru", "settings") => "Настройки",
        ("ru", "quit") => "Выход",
        ("es", "open") => "Abrir traductor",
        ("es", "ocr") => "Traducir área de pantalla",
        ("es", "clipboard") => "Traducir portapapeles",
        ("es", "pause") => "Pausar traducción automática",
        ("es", "history") => "Historial",
        ("es", "settings") => "Ajustes",
        ("es", "quit") => "Salir",
        (_, "open") => "Open translator",
        (_, "ocr") => "Translate screen area",
        (_, "clipboard") => "Translate clipboard",
        (_, "pause") => "Pause auto-translation",
        (_, "history") => "History",
        (_, "settings") => "Settings",
        _ => "Quit",
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let l = ui_lang(app);
    let paused = app.state_ref().settings().paused;
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", tr(&l, "open"), true, None::<&str>)?,
            &MenuItem::with_id(app, "ocr", tr(&l, "ocr"), true, None::<&str>)?,
            &MenuItem::with_id(app, "clipboard", tr(&l, "clipboard"), true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(app, "pause", tr(&l, "pause"), true, paused, None::<&str>)?,
            &MenuItem::with_id(app, "history", tr(&l, "history"), true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", tr(&l, "settings"), true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", tr(&l, "quit"), true, None::<&str>)?,
        ],
    )
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("1lang")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => windows::show_main(app),
            "ocr" => actions::start_ocr(app.clone()),
            "clipboard" => actions::translate_clipboard(app.clone(), false),
            "pause" => {
                let paused = !app.state_ref().settings().paused;
                crate::commands::set_paused(app.clone(), paused);
            }
            "history" => crate::commands::open_main(app.clone(), Some("history".into())),
            "settings" => crate::commands::open_main(app.clone(), Some("settings".into())),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                windows::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuild the menu (language / paused state changed).
pub fn refresh(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}
