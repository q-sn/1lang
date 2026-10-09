//! Typed events (exported to TypeScript by tauri-specta).

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::settings::Settings;
use crate::state::PopupPayload;

/// The popup has new content.
#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct PopupShow(pub PopupPayload);

#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct PopupHidden;

#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct SettingsChanged(pub Settings);

/// A region-picker window should display a frozen screenshot.
#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct RegionShow {
    pub index: u32,
    pub url: String,
    pub scale_factor: f64,
}

/// Put text into the main window's source field.
#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct MainOpenText {
    pub text: String,
    pub translate: bool,
}

/// Switch the main window to a view ("translate" | "history" | "settings").
#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct MainNavigate(pub String);

#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct IconBusy(pub bool);

#[derive(Serialize, Deserialize, Type, Clone, Debug, Event)]
pub struct HistoryChanged;
