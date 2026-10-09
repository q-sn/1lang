//! Commands callable from the frontend (typed via tauri-specta).

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use onelang_core::providers::ProviderKind;
use onelang_core::{detect, lang, TranslateEvent, TranslateRequest, TranslationResult};
use onelang_platform::{clipboard, input, ocr, system};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::Channel;
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_autostart::ManagerExt;
use tauri_specta::Event;

use crate::actions;
use crate::db::{HistoryItem, HistoryQuery, Origin, UsageStats};
use crate::events::{HistoryChanged, MainNavigate, MainOpenText, RegionShow, SettingsChanged};
use crate::secrets;
use crate::service::{self, RunOptions};
use crate::settings::Settings;
use crate::state::{AppExt, PopupPayload};
use crate::triggers;
use crate::windows;

#[derive(Serialize, Type)]
pub struct AppInfo {
    pub version: String,
    pub system_language: String,
    pub is_windows_11: bool,
    pub ocr_languages: Vec<String>,
    pub accent_colors: Vec<String>,
    pub languages: Vec<String>,
}

#[tauri::command(async)]
#[specta::specta]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        system_language: crate::settings::system_language(),
        is_windows_11: app.state_ref().win11,
        ocr_languages: ocr::available_languages().unwrap_or_default(),
        accent_colors: system::accent_colors().unwrap_or_default(),
        languages: lang::all_codes(),
    }
}

/// Whether the calling window has a native glass background.
#[tauri::command]
#[specta::specta]
pub fn window_glass(app: AppHandle, window: WebviewWindow) -> bool {
    windows::has_glass(&app, window.label())
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(app: AppHandle) -> Settings {
    app.state_ref().settings()
}

#[derive(Serialize, Type)]
pub struct SettingsUpdate {
    pub settings: Settings,
    pub warnings: Vec<String>,
}

/// Send settings to every window except `except` (the one that saved them,
/// otherwise the echo would overwrite what the user is typing there).
pub fn broadcast_settings(app: &AppHandle, s: &Settings, except: Option<&str>) {
    let except = except.map(str::to_string);
    let _ = SettingsChanged(s.clone()).emit_filter(app, move |t| match (&except, t) {
        (Some(ex), tauri::EventTarget::WebviewWindow { label })
        | (Some(ex), tauri::EventTarget::Webview { label })
        | (Some(ex), tauri::EventTarget::Window { label }) => label != ex,
        _ => true,
    });
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_settings(app: AppHandle, window: WebviewWindow, settings: Settings) -> Result<SettingsUpdate, String> {
    apply_settings(&app, settings, Some(window.label()))
}

pub fn apply_settings(app: &AppHandle, settings: Settings, from: Option<&str>) -> Result<SettingsUpdate, String> {
    let app = app.clone();
    let state = app.state_ref();
    let old = state.settings();
    let mut new = settings;
    // The pinned popup position is owned by the backend.
    new.popup.last_x = old.popup.last_x;
    new.popup.last_y = old.popup.last_y;
    new.languages.primary = lang::normalize(&new.languages.primary);
    new.languages.secondary = lang::normalize(&new.languages.secondary);
    state.settings_file.save(&new).map_err(|e| e.to_string())?;
    *state.settings.write().unwrap() = new.clone();

    let mut warnings = vec![];
    if old.hotkeys != new.hotkeys {
        warnings.extend(triggers::register_hotkeys(&app, &new));
    }
    if old.selection != new.selection || old.paused != new.paused {
        triggers::apply(&app);
    }
    if old.general.theme != new.general.theme || old.general.window_effect != new.general.window_effect {
        windows::apply_effects_all(&app);
    }
    if old.general.autostart != new.general.autostart {
        let al = app.autolaunch();
        let r = if new.general.autostart { al.enable() } else { al.disable() };
        if let Err(e) = r {
            warnings.push(format!("autostart: {e}"));
        }
    }
    crate::tray::refresh(&app);
    broadcast_settings(&app, &new, from);
    Ok(SettingsUpdate { settings: new, warnings })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_paused(app: AppHandle, paused: bool) {
    let state = app.state_ref();
    let s = {
        let mut s = state.settings.write().unwrap();
        s.paused = paused;
        s.clone()
    };
    let _ = state.settings_file.save(&s);
    triggers::apply(&app);
    crate::tray::refresh(&app);
    broadcast_settings(&app, &s, None);
}

#[derive(Serialize, Type)]
pub struct ProviderKindInfo {
    pub kind: ProviderKind,
    pub label: String,
    pub is_llm: bool,
    pub needs_key: bool,
    pub default_base_url: Option<String>,
    pub default_model: Option<String>,
    pub recommended_models: Vec<onelang_core::providers::RecommendedModel>,
}

#[tauri::command]
#[specta::specta]
pub fn provider_kinds() -> Vec<ProviderKindInfo> {
    ProviderKind::ALL
        .iter()
        .map(|k| ProviderKindInfo {
            kind: *k,
            label: k.label().into(),
            is_llm: k.is_llm(),
            needs_key: k.needs_key(),
            default_base_url: k.default_base_url().map(Into::into),
            default_model: k.default_model().map(Into::into),
            recommended_models: k.recommended_models(),
        })
        .collect()
}

#[derive(Serialize, Type)]
pub struct KeyStatus {
    pub id: String,
    pub hint: Option<String>,
}

#[tauri::command(async)]
#[specta::specta]
pub fn key_statuses(app: AppHandle) -> Vec<KeyStatus> {
    app.state_ref()
        .settings()
        .providers
        .iter()
        .map(|p| KeyStatus { id: p.id.clone(), hint: secrets::hint(&p.id) })
        .collect()
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_api_key(provider_id: String, key: Option<String>) -> Result<(), String> {
    secrets::set(&provider_id, key.as_deref())
}

#[tauri::command]
#[specta::specta]
pub async fn list_models(app: AppHandle, provider_id: String) -> Result<Vec<String>, String> {
    let state = app.state_ref();
    let settings = state.settings();
    let p = service::build_provider(&state, &settings, &provider_id)?;
    p.list_models().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn test_provider(app: AppHandle, provider_id: String) -> Result<TranslationResult, String> {
    let state = app.state_ref();
    let settings = state.settings();
    let engine = service::build_engine(&state, &settings, Some(&provider_id))?;
    let target = if settings.languages.primary == "en" { "es" } else { settings.languages.primary.as_str() };
    engine
        .translate(
            TranslateRequest {
                text: "The quick brown fox jumps over the lazy dog.".into(),
                source_lang: Some("en".into()),
                target_lang: Some(target.into()),
                mode: onelang_core::TranslateMode::Translate,
                provider_id: Some(provider_id),
                ..Default::default()
            },
            &|_| {},
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn translate(
    app: AppHandle,
    request: TranslateRequest,
    request_id: String,
    origin: Origin,
    app_exe: Option<String>,
    save_history: bool,
    on_event: Channel<TranslateEvent>,
) -> Result<TranslationResult, String> {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    app.state_ref().tasks.lock().unwrap().insert(request_id.clone(), cancel_tx);
    let emit: Arc<dyn Fn(TranslateEvent) + Send + Sync> = Arc::new(move |e| {
        let _ = on_event.send(e);
    });
    let result = tokio::select! {
        r = service::run(&app, request, RunOptions { origin, app: app_exe, save_history }, emit) => r,
        _ = cancel_rx => Err("cancelled".to_string()),
    };
    app.state_ref().tasks.lock().unwrap().remove(&request_id);
    if result.is_ok() && save_history {
        let _ = HistoryChanged.emit(&app);
    }
    result
}

#[tauri::command]
#[specta::specta]
pub fn cancel_translation(app: AppHandle, request_id: String) {
    if let Some(tx) = app.state_ref().tasks.lock().unwrap().remove(&request_id) {
        let _ = tx.send(());
    }
}

#[tauri::command]
#[specta::specta]
pub fn detect_language(app: AppHandle, text: String) -> Option<String> {
    let s = app.state_ref().settings();
    detect::detect(&text, &s.languages.favorites).filter(|d| d.is_reliable()).map(|d| d.lang)
}

#[tauri::command(async)]
#[specta::specta]
pub fn history_list(app: AppHandle, query: HistoryQuery) -> Result<Vec<HistoryItem>, String> {
    app.state_ref().db.lock().unwrap().list_history(&query).map_err(|e| e.to_string())
}

/// Save a translation made in the main window (once the user stopped typing).
#[tauri::command(async)]
#[specta::specta]
pub fn history_commit(app: AppHandle, source_text: String, result: TranslationResult) -> Result<(), String> {
    let state = app.state_ref();
    let s = state.settings();
    if !s.translation.history {
        return Ok(());
    }
    state
        .db
        .lock()
        .unwrap()
        .add_history(source_text.trim(), &result, Origin::Main, None, s.translation.history_limit)
        .map_err(|e| e.to_string())?;
    let _ = HistoryChanged.emit(&app);
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn history_toggle_favorite(app: AppHandle, id: i32) -> Result<(), String> {
    app.state_ref().db.lock().unwrap().toggle_favorite(id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta::specta]
pub fn history_delete(app: AppHandle, id: i32) -> Result<(), String> {
    app.state_ref().db.lock().unwrap().delete_history(id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta::specta]
pub fn history_clear(app: AppHandle, keep_favorites: bool) -> Result<(), String> {
    app.state_ref().db.lock().unwrap().clear_history(keep_favorites).map_err(|e| e.to_string())?;
    let _ = HistoryChanged.emit(&app);
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn usage_stats(app: AppHandle) -> Result<UsageStats, String> {
    app.state_ref().db.lock().unwrap().usage_stats().map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta::specta]
pub fn usage_reset(app: AppHandle) -> Result<(), String> {
    app.state_ref().db.lock().unwrap().reset_usage().map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta::specta]
pub fn cache_clear(app: AppHandle) -> Result<(), String> {
    app.state_ref().db.lock().unwrap().clear_cache().map_err(|e| e.to_string())
}

// ---------- popup ----------

#[tauri::command]
#[specta::specta]
pub fn popup_take_pending(app: AppHandle) -> Option<PopupPayload> {
    app.state_ref().popup.lock().unwrap().pending.clone()
}

#[tauri::command]
#[specta::specta]
pub fn popup_set_pinned(app: AppHandle, pinned: bool) {
    app.state_ref().popup.lock().unwrap().pinned = pinned;
}

#[tauri::command]
#[specta::specta]
pub fn popup_hide(app: AppHandle) {
    windows::hide_popup(&app);
}

#[tauri::command]
#[specta::specta]
pub fn open_in_main(app: AppHandle, text: String) {
    windows::hide_popup(&app);
    windows::show_main(&app);
    let _ = MainNavigate("translate".into()).emit_to(&app, windows::MAIN);
    let _ = MainOpenText { text, translate: true }.emit_to(&app, windows::MAIN);
}

/// Replace the selection in the app the popup was opened from.
#[tauri::command]
#[specta::specta]
pub fn replace_with(app: AppHandle, text: String) -> Result<(), String> {
    let hwnd = app.state_ref().popup.lock().unwrap().source_hwnd;
    windows::hide_popup(&app);
    thread::spawn(move || {
        if hwnd != 0 {
            onelang_platform::window::force_foreground(hwnd);
        }
        thread::sleep(Duration::from_millis(120));
        app.state_ref()
            .suppress_clipboard_until
            .store(crate::db::now_ms() + 2000, std::sync::atomic::Ordering::Relaxed);
        if let Err(e) = input::paste_text(&text) {
            log::error!("paste: {e}");
        }
    });
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.state_ref()
        .suppress_clipboard_until
        .store(crate::db::now_ms() + 500, std::sync::atomic::Ordering::Relaxed);
    clipboard::write_text(&text, false).map_err(|e| e.to_string())
}

#[tauri::command(async)]
#[specta::specta]
pub fn exclude_app(app: AppHandle, exe: String) -> Result<(), String> {
    let mut s = app.state_ref().settings();
    let exe = exe.trim().to_lowercase();
    if !exe.is_empty() && !s.selection.excluded_apps.iter().any(|e| e.eq_ignore_ascii_case(&exe)) {
        s.selection.excluded_apps.push(exe);
        apply_settings(&app, s, None)?;
    }
    Ok(())
}

// ---------- icon / OCR / misc ----------

#[tauri::command]
#[specta::specta]
pub fn icon_clicked(app: AppHandle) {
    thread::spawn(move || actions::icon_clicked(&app));
}

#[tauri::command(async)]
#[specta::specta]
pub fn region_init(app: AppHandle, window: WebviewWindow) -> Option<RegionShow> {
    let index: u32 = window.label().strip_prefix("region-")?.parse().ok()?;
    actions::region_payload(&app, index)
}

#[tauri::command(async)]
#[specta::specta]
pub fn region_ready(app: AppHandle, index: u32) {
    actions::region_ready(&app, index);
}

#[derive(Serialize, Deserialize, Type)]
pub struct RegionRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
#[specta::specta]
pub fn region_selected(app: AppHandle, index: u32, rect: RegionRect) {
    actions::region_selected(app, index, rect.x, rect.y, rect.width, rect.height);
}

#[tauri::command]
#[specta::specta]
pub fn region_cancel(app: AppHandle) {
    actions::region_cancel(&app);
}

#[tauri::command]
#[specta::specta]
pub fn start_ocr(app: AppHandle) {
    actions::start_ocr_from_main(app);
}

#[tauri::command]
#[specta::specta]
pub fn open_main(app: AppHandle, view: Option<String>) {
    windows::show_main(&app);
    if let Some(v) = view {
        let _ = MainNavigate(v).emit_to(&app, windows::MAIN);
    }
}

#[tauri::command]
#[specta::specta]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

// ---------- updates ----------

#[tauri::command]
#[specta::specta]
pub fn update_status(app: AppHandle) -> crate::updater::UpdateStatus {
    crate::updater::status(&app)
}

/// Check now (and download if there is a newer version).
#[tauri::command]
#[specta::specta]
pub async fn check_updates(app: AppHandle) -> crate::updater::UpdateStatus {
    crate::updater::check_and_download(&app).await
}

/// Install the downloaded update right away and start the new version.
#[tauri::command]
#[specta::specta]
pub fn restart_to_update(app: AppHandle) -> bool {
    crate::updater::install_pending(&app, true)
}
