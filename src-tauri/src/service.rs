//! Glue between the UI and onelang-core: builds the provider chain from
//! settings + secrets, adds caching, history and usage accounting.

use std::sync::Arc;

use onelang_core::engine::EngineOptions;
use onelang_core::providers::{self, Provider};
use onelang_core::{Engine, Error as CoreError, LanguagePair, TranslateEvent, TranslateRequest, TranslationResult};
use tauri::AppHandle;

use crate::db::{cache_key, Origin};
use crate::secrets;
use crate::settings::Settings;
use crate::state::{AppExt, AppState};

const CACHE_TTL_MS: i64 = 30 * 24 * 3600 * 1000;

pub fn build_provider(state: &AppState, settings: &Settings, id: &str) -> Result<Arc<dyn Provider>, String> {
    let cfg = settings
        .providers
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("Unknown provider {id}"))?
        .clone();
    providers::build(cfg, secrets::get(id), state.http.clone()).map_err(|e| e.to_string())
}

pub fn build_engine(state: &AppState, settings: &Settings, only: Option<&str>) -> Result<Engine, String> {
    let mut list: Vec<Arc<dyn Provider>> = vec![];
    let mut skipped: Vec<String> = vec![];
    for cfg in &settings.providers {
        let wanted = match only {
            Some(id) => cfg.id == id,
            None => cfg.enabled,
        };
        if !wanted {
            continue;
        }
        match providers::build(cfg.clone(), secrets::get(&cfg.id), state.http.clone()) {
            Ok(p) => list.push(p),
            Err(e) => {
                log::info!("provider {} skipped: {e}", cfg.name);
                skipped.push(e.to_string());
            }
        }
    }
    if list.is_empty() {
        return Err(skipped.into_iter().next().unwrap_or_else(|| CoreError::NoProviders.to_string()));
    }
    let custom = settings.translation.custom_instructions.trim();
    Ok(Engine::new(
        list,
        EngineOptions {
            pair: LanguagePair {
                primary: settings.languages.primary.clone(),
                secondary: settings.languages.secondary.clone(),
            },
            detection: settings.languages.detection,
            favorite_languages: settings.languages.favorites.clone(),
            custom_instructions: (!custom.is_empty()).then(|| custom.to_string()),
            dictionary_for_words: settings.translation.dictionary_for_words,
        },
    ))
}

fn request_cache_key(settings: &Settings, req: &TranslateRequest) -> String {
    let chain: Vec<String> = settings
        .providers
        .iter()
        .filter(|p| match &req.provider_id {
            Some(id) => &p.id == id,
            None => p.enabled,
        })
        .map(|p| format!("{}:{}", p.id, p.model().unwrap_or_default()))
        .collect();
    cache_key(&[
        &chain.join(","),
        &req.text,
        req.source_lang.as_deref().unwrap_or("auto"),
        req.target_lang.as_deref().unwrap_or("auto"),
        &format!("{:?}{:?}{}", req.mode, req.formality, settings.translation.dictionary_for_words),
        req.context.as_deref().unwrap_or(""),
        &settings.translation.custom_instructions,
        &settings.languages.primary,
        &settings.languages.secondary,
        &format!("{:?}", settings.languages.detection),
    ])
}

pub struct RunOptions {
    pub origin: Origin,
    pub app: Option<String>,
    pub save_history: bool,
}

/// Translate and emit every event (including `Done` / `Error`) through `emit`.
pub async fn run(
    app: &AppHandle,
    mut req: TranslateRequest,
    opts: RunOptions,
    emit: Arc<dyn Fn(TranslateEvent) + Send + Sync>,
) -> Result<TranslationResult, String> {
    let state = app.state_ref();
    let settings = state.settings();
    if !settings.translation.use_context {
        req.context = None;
    }
    let source_text = req.text.trim().to_string();

    let key = request_cache_key(&settings, &req);
    if settings.translation.cache {
        let cached = state.db.lock().unwrap().cache_get(&key, CACHE_TTL_MS);
        if let Some(mut r) = cached {
            r.cached = true;
            r.elapsed_ms = 0;
            r.cost_usd = Some(0.0);
            emit(TranslateEvent::Started {
                provider_id: r.provider_id.clone(),
                provider_name: r.provider_name.clone(),
                model: r.model.clone(),
                source_lang: r.source_lang.clone(),
                target_lang: Some(r.target_lang.clone()),
                kind: r.kind,
            });
            emit(TranslateEvent::Done { result: Box::new(r.clone()) });
            save_history(&state, &settings, &source_text, &r, &opts);
            return Ok(r);
        }
    }

    let engine = match build_engine(&state, &settings, req.provider_id.as_deref()) {
        Ok(e) => e,
        Err(e) => {
            emit(TranslateEvent::Error { message: e.clone() });
            return Err(e);
        }
    };
    let emit_inner = emit.clone();
    match engine.translate(req, &move |e| emit_inner(e)).await {
        Ok(r) => {
            {
                let db = state.db.lock().unwrap();
                if let Err(e) = db.record_usage(&r) {
                    log::warn!("usage: {e}");
                }
                if settings.translation.cache {
                    let _ = db.cache_put(&key, &r);
                }
            }
            save_history(&state, &settings, &source_text, &r, &opts);
            emit(TranslateEvent::Done { result: Box::new(r.clone()) });
            Ok(r)
        }
        Err(e) => {
            let msg = e.to_string();
            emit(TranslateEvent::Error { message: msg.clone() });
            Err(msg)
        }
    }
}

fn save_history(state: &AppState, settings: &Settings, source: &str, r: &TranslationResult, opts: &RunOptions) {
    if !opts.save_history || !settings.translation.history {
        return;
    }
    let db = state.db.lock().unwrap();
    if let Err(e) = db.add_history(source, r, opts.origin, opts.app.as_deref(), settings.translation.history_limit) {
        log::warn!("history: {e}");
    }
}
