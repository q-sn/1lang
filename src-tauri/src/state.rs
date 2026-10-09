use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU32};
use std::sync::{Mutex, RwLock};

use onelang_platform::capture::MonitorShot;
use onelang_platform::{Rect, Selection};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::db::{Db, Origin};
use crate::settings::{Settings, SettingsFile};
use crate::triggers::TriggerRuntime;

/// What the popup should show.
#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct PopupPayload {
    pub id: u32,
    pub text: String,
    pub context: Option<String>,
    pub origin: Origin,
    /// Executable of the app the text came from.
    pub app: Option<String>,
    /// Start translating immediately.
    pub translate: bool,
    /// The selection can be replaced with the translation.
    pub can_replace: bool,
    /// Message to show instead of a translation (e.g. "nothing selected").
    pub notice: Option<String>,
}

#[derive(Default)]
pub struct PopupState {
    pub pending: Option<PopupPayload>,
    pub pinned: bool,
    pub visible: bool,
    /// Window that had focus before the popup, for "replace selection".
    pub source_hwnd: isize,
    pub shown_at: i64,
}

#[derive(Default)]
pub struct IconState {
    pub visible: bool,
    pub selection: Option<Selection>,
    pub app: Option<String>,
    /// Physical screen rect of the icon window.
    pub rect: Rect,
    /// Used as a progress indicator by "replace selection": not clickable.
    pub busy: bool,
    pub generation: u64,
}

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_file: SettingsFile,
    pub db: Mutex<Db>,
    pub http: reqwest::Client,
    /// Running translations by request id; sending cancels them.
    pub tasks: Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>,
    pub popup: Mutex<PopupState>,
    pub icon: Mutex<IconState>,
    pub captures: Mutex<Vec<MonitorShot>>,
    pub triggers: Mutex<Option<TriggerRuntime>>,
    /// Clipboard changes made by 1lang itself before this time (ms) are ignored.
    pub suppress_clipboard_until: AtomicI64,
    pub popup_counter: AtomicU32,
    /// The OCR region picker is open.
    pub ocr_active: std::sync::atomic::AtomicBool,
    /// OCR was started from the main window: it is hidden during capture and
    /// receives the recognized text instead of the popup.
    pub ocr_to_main: std::sync::atomic::AtomicBool,
    /// Windows 11: Mica / Acrylic are available.
    pub win11: bool,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }
}

pub trait AppExt {
    fn state_ref(&self) -> tauri::State<'_, AppState>;
}

impl AppExt for AppHandle {
    fn state_ref(&self) -> tauri::State<'_, AppState> {
        self.state::<AppState>()
    }
}
