mod actions;
mod commands;
mod db;
mod events;
mod secrets;
mod service;
mod settings;
mod state;
mod tray;
mod triggers;
mod updater;
mod windows;

use std::sync::atomic::{AtomicI64, AtomicU32};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use tauri::http::Response;
use tauri::{Manager, RunEvent};
use tauri_specta::{collect_commands, collect_events};

use crate::state::{AppExt, AppState};

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::app_info,
            commands::window_glass,
            commands::get_settings,
            commands::update_settings,
            commands::set_paused,
            commands::provider_kinds,
            commands::key_statuses,
            commands::set_api_key,
            commands::list_models,
            commands::test_provider,
            commands::translate,
            commands::cancel_translation,
            commands::detect_language,
            commands::history_list,
            commands::history_commit,
            commands::history_toggle_favorite,
            commands::history_delete,
            commands::history_clear,
            commands::usage_stats,
            commands::usage_reset,
            commands::cache_clear,
            commands::popup_take_pending,
            commands::popup_set_pinned,
            commands::popup_hide,
            commands::open_in_main,
            commands::replace_with,
            commands::copy_text,
            commands::exclude_app,
            commands::icon_clicked,
            commands::region_init,
            commands::region_ready,
            commands::region_selected,
            commands::region_cancel,
            commands::start_ocr,
            commands::open_main,
            commands::quit,
            commands::update_status,
            commands::check_updates,
            commands::restart_to_update,
        ])
        .events(collect_events![
            events::PopupShow,
            events::PopupHidden,
            events::SettingsChanged,
            events::RegionShow,
            events::MainOpenText,
            events::MainNavigate,
            events::IconBusy,
            events::HistoryChanged,
            updater::UpdateStatus,
        ])
}

/// Write `src/bindings.ts` (also done by the `export_bindings` test).
pub fn export_bindings(path: &str) {
    specta_builder()
        .export(
            specta_typescript::Typescript::default().header("// @ts-nocheck\n/* eslint-disable */"),
            path,
        )
        .expect("failed to export TypeScript bindings");
}

pub fn run() {
    // `cargo run -p onelang -- --export-bindings` (test binaries can't load
    // Tauri on Windows without the app manifest, so this is a CLI flag).
    if std::env::args().any(|a| a == "--export-bindings") {
        export_bindings("../src/bindings.ts");
        return;
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,onelang_lib=debug,onelang_core=debug,onelang_platform=debug")).init();

    let builder = specta_builder();
    #[cfg(debug_assertions)]
    export_bindings("../src/bindings.ts");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        // Frozen screenshots for the OCR region picker: http://capture.localhost/<monitor>
        .register_asynchronous_uri_scheme_protocol("capture", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let index: Option<usize> = request.uri().path().trim_start_matches('/').parse().ok();
            std::thread::spawn(move || {
                let image = index.and_then(|i| {
                    let state = app.state_ref();
                    let caps = state.captures.lock().unwrap();
                    caps.get(i).map(|s| s.image.clone())
                });
                let bytes = image.and_then(|img| onelang_platform::capture::encode_bmp(&img).ok());
                let resp = match bytes {
                    Some(b) => Response::builder()
                        .header("Content-Type", "image/bmp")
                        .body(b),
                    None => Response::builder().status(404).body(Vec::new()),
                };
                responder.respond(resp.unwrap());
            });
        })
        .setup(move |app| {
            builder.mount_events(app);
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let settings_file = settings::SettingsFile::new(config_dir);
            let settings = settings_file.load();
            let db = db::Db::open(&data_dir.join("1lang.db")).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(8))
                .read_timeout(Duration::from_secs(30))
                .user_agent(concat!("1lang/", env!("CARGO_PKG_VERSION")))
                .build()?;

            app.manage(AppState {
                settings: RwLock::new(settings.clone()),
                settings_file,
                db: Mutex::new(db),
                http,
                tasks: Mutex::new(Default::default()),
                popup: Mutex::new(Default::default()),
                icon: Mutex::new(Default::default()),
                captures: Mutex::new(vec![]),
                triggers: Mutex::new(None),
                suppress_clipboard_until: AtomicI64::new(0),
                popup_counter: AtomicU32::new(0),
                ocr_active: Default::default(),
                ocr_to_main: Default::default(),
                win11: windows::is_windows_11(),
            });

            // Pre-create windows so they appear instantly when needed.
            let main = windows::ensure_main(&handle)?;
            windows::ensure_popup(&handle)?;
            windows::ensure_icon(&handle)?;
            tray::create(&handle)?;

            let started_by_autostart = std::env::args().any(|a| a == "--autostart");
            if settings.general.show_main_on_start && !started_by_autostart {
                let _ = main.show();
                let _ = main.set_focus();
            }

            for w in triggers::register_hotkeys(&handle, &settings) {
                log::warn!("{w}");
            }
            triggers::apply(&handle);
            app.manage(updater::UpdaterState::default());
            updater::spawn(handle.clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building 1lang");

    app.run(|app, event| match event {
        // Keep running in the tray when all windows are hidden/closed.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        // A downloaded update is installed when the app really quits.
        RunEvent::Exit => {
            updater::install_pending(app, false);
        }
        _ => {}
    });
}
