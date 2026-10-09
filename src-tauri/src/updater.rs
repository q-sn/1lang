//! Silent auto-update from GitHub Releases.
//!
//! The update is checked and downloaded in the background; it is installed
//! when the app exits (tray "Quit", logout, shutdown), so the next launch is
//! the new version and nothing is interrupted. A manual "restart now" is
//! available from the settings.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::db::now_ms;
use crate::state::AppExt;

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Downloading,
    /// Downloaded; will be installed when the app exits.
    Ready,
    UpToDate,
    Error,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default, Event)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub phase: UpdatePhase,
    pub error: Option<String>,
    /// Unix ms of the last finished check.
    pub checked_at: Option<f64>,
}

#[derive(Default)]
pub struct UpdaterState {
    status: Mutex<UpdateStatus>,
    pending: Mutex<Option<(Update, Vec<u8>)>>,
    busy: Mutex<bool>,
}

fn set_status(app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) {
    let st = app.state::<UpdaterState>();
    let snapshot = {
        let mut s = st.status.lock().unwrap();
        f(&mut s);
        s.clone()
    };
    let _ = snapshot.emit(app);
}

pub fn status(app: &AppHandle) -> UpdateStatus {
    let mut s = app.state::<UpdaterState>().status.lock().unwrap().clone();
    s.current = app.package_info().version.to_string();
    s
}

/// Check GitHub and download a newer version if there is one.
pub async fn check_and_download(app: &AppHandle) -> UpdateStatus {
    {
        let st = app.state::<UpdaterState>();
        let mut busy = st.busy.lock().unwrap();
        if *busy || st.pending.lock().unwrap().is_some() {
            return status(app);
        }
        *busy = true;
    }
    set_status(app, |s| {
        s.phase = UpdatePhase::Checking;
        s.error = None;
    });

    let result: Result<Option<String>, String> = async {
        let updater = app
            .updater_builder()
            .restart_after_install(false)
            .build()
            .map_err(|e| e.to_string())?;
        let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let version = update.version.clone();
        set_status(app, |s| {
            s.phase = UpdatePhase::Downloading;
            s.latest = Some(version.clone());
        });
        let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
        *app.state::<UpdaterState>().pending.lock().unwrap() = Some((update, bytes));
        Ok(Some(version))
    }
    .await;

    *app.state::<UpdaterState>().busy.lock().unwrap() = false;
    set_status(app, |s| {
        s.checked_at = Some(now_ms() as f64);
        match &result {
            Ok(Some(v)) => {
                log::info!("update {v} downloaded, will be installed on exit");
                s.phase = UpdatePhase::Ready;
                s.latest = Some(v.clone());
            }
            Ok(None) => {
                s.phase = UpdatePhase::UpToDate;
                s.latest = None;
            }
            Err(e) => {
                log::warn!("update check failed: {e}");
                s.phase = UpdatePhase::Error;
                s.error = Some(e.clone());
            }
        }
    });
    status(app)
}

/// Background loop: first check shortly after start, then every few hours.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            if app.state_ref().settings().general.auto_update && !cfg!(debug_assertions) {
                check_and_download(&app).await;
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// Install a downloaded update. `restart` = start the new version afterwards.
/// Returns false when there is nothing to install.
pub fn install_pending(app: &AppHandle, restart: bool) -> bool {
    let Some((update, bytes)) = app.state::<UpdaterState>().pending.lock().unwrap().take() else {
        return false;
    };
    let update = update.restart_after_install(restart);
    match update.install(bytes) {
        Ok(()) => true,
        Err(e) => {
            log::error!("installing update failed: {e}");
            false
        }
    }
}
