//! User settings, stored as JSON in the app config directory.

use std::path::PathBuf;

use onelang_core::providers::{ProviderConfig, ProviderKind};
use onelang_core::{lang, DetectionMode, Formality};
use onelang_platform::SelectionMethod;
use serde::{Deserialize, Serialize};
use specta::Type;

pub const SETTINGS_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WindowEffect {
    /// Mica / Acrylic on Windows 11, solid translucent background elsewhere.
    #[default]
    Auto,
    Mica,
    Acrylic,
    None,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// Near the selected text when its position is known, otherwise near the cursor.
    #[default]
    Selection,
    Cursor,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PopupPlacement {
    #[default]
    Selection,
    Cursor,
    /// Where the pinned popup was last left.
    LastPosition,
    ScreenCenter,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoubleCopyDetection {
    /// Watch clipboard changes while Ctrl is held. No keyboard hook.
    #[default]
    Clipboard,
    /// Low-level keyboard hook.
    Keyboard,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct GeneralSettings {
    /// "system" or a UI language code (ru / en / es).
    pub ui_language: String,
    pub theme: ThemeMode,
    pub window_effect: WindowEffect,
    pub autostart: bool,
    /// Open the main window when the app starts (otherwise only the tray icon).
    pub show_main_on_start: bool,
    /// Closing the main window keeps the app running in the tray.
    pub close_to_tray: bool,
    /// Download updates in the background and install them on exit.
    pub auto_update: bool,
    /// Interface zoom, percent.
    pub ui_scale: u32,
    /// Font size of source / translated text, px (at 100% zoom).
    pub font_size: u32,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            ui_language: "system".into(),
            theme: ThemeMode::System,
            window_effect: WindowEffect::None,
            autostart: false,
            show_main_on_start: true,
            close_to_tray: true,
            auto_update: true,
            ui_scale: 100,
            font_size: 17,
        }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LanguageSettings {
    /// Text in any other language is translated into this one.
    pub primary: String,
    /// Text already in `primary` is translated into this one.
    pub secondary: String,
    /// Shown first in language pickers and used to disambiguate detection.
    pub favorites: Vec<String>,
    pub detection: DetectionMode,
}

impl Default for LanguageSettings {
    fn default() -> Self {
        let primary = system_language();
        let secondary = if primary == "en" { "es".to_string() } else { "en".to_string() };
        let mut favorites = vec![primary.clone(), secondary.clone()];
        for l in ["ru", "en", "es"] {
            if !favorites.iter().any(|f| f == l) {
                favorites.push(l.to_string());
            }
        }
        Self { primary, secondary, favorites, detection: DetectionMode::Local }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct TranslationSettings {
    pub formality: Formality,
    /// Extra instructions for LLM providers (style, terminology).
    pub custom_instructions: String,
    /// Show a dictionary card for single words (LLM providers).
    pub dictionary_for_words: bool,
    /// Send surrounding text of the selection to the LLM for better accuracy.
    pub use_context: bool,
    pub cache: bool,
    pub history: bool,
    pub history_limit: u32,
}

impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            formality: Formality::Default,
            custom_instructions: String::new(),
            dictionary_for_words: true,
            use_context: true,
            cache: true,
            history: true,
            history_limit: 5000,
        }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SelectionSettings {
    /// How the hotkey reads selected text.
    pub method: SelectionMethod,
    /// Put the previous clipboard content back after reading the selection via Ctrl+C.
    pub restore_clipboard: bool,

    /// Show a floating button after selecting text with the mouse.
    pub icon_enabled: bool,
    pub icon_anchor: Anchor,
    /// Hide the button after this many ms (0 = until the next click).
    pub icon_timeout_ms: u32,
    /// The button may also read the selection via Ctrl+C when UI Automation fails.
    /// Off by default: it would touch the clipboard after every mouse selection.
    pub icon_clipboard_fallback: bool,

    /// Ctrl+C+C translates the copied text.
    pub double_copy_enabled: bool,
    pub double_copy_detection: DoubleCopyDetection,
    pub double_copy_interval_ms: u32,

    /// Executables (e.g. "code.exe") where automatic triggers are disabled.
    pub excluded_apps: Vec<String>,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        Self {
            method: SelectionMethod::UiaThenClipboard,
            restore_clipboard: true,
            icon_enabled: true,
            icon_anchor: Anchor::Selection,
            icon_timeout_ms: 4000,
            icon_clipboard_fallback: false,
            double_copy_enabled: true,
            double_copy_detection: DoubleCopyDetection::Clipboard,
            double_copy_interval_ms: 450,
            excluded_apps: vec!["keepass.exe".into(), "1password.exe".into(), "bitwarden.exe".into()],
        }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct HotkeySettings {
    /// Accelerators in global-hotkey format, e.g. "Ctrl+Alt+KeyT". Empty = disabled.
    pub translate_selection: String,
    pub replace_selection: String,
    pub ocr_region: String,
    pub translate_clipboard: String,
    pub open_main: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            translate_selection: "Ctrl+Alt+KeyT".into(),
            replace_selection: "Ctrl+Alt+KeyR".into(),
            ocr_region: "Ctrl+Alt+KeyO".into(),
            translate_clipboard: String::new(),
            open_main: "Ctrl+Alt+KeyL".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct PopupSettings {
    pub placement: PopupPlacement,
    pub close_on_blur: bool,
    /// Every popup opens pinned.
    pub always_pinned: bool,
    pub width: u32,
    pub height: u32,
    /// Logical position of the pinned popup.
    pub last_x: Option<i32>,
    pub last_y: Option<i32>,
}

impl Default for PopupSettings {
    fn default() -> Self {
        Self {
            placement: PopupPlacement::Selection,
            close_on_blur: true,
            always_pinned: false,
            width: 600,
            height: 180,
            last_x: None,
            last_y: None,
        }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct OcrSettings {
    /// BCP-47 tag of a Windows OCR language, or "auto".
    pub language: String,
    /// Translate right away (otherwise only show the recognized text).
    pub translate: bool,
}

impl Default for OcrSettings {
    fn default() -> Self {
        Self { language: "auto".into(), translate: true }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub general: GeneralSettings,
    pub languages: LanguageSettings,
    /// Fallback chain: enabled providers are tried in this order.
    pub providers: Vec<ProviderConfig>,
    pub translation: TranslationSettings,
    pub selection: SelectionSettings,
    pub hotkeys: HotkeySettings,
    pub popup: PopupSettings,
    pub ocr: OcrSettings,
    /// Automatic triggers (button, Ctrl+C+C) are paused from the tray.
    pub paused: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            general: Default::default(),
            languages: Default::default(),
            providers: vec![
                ProviderConfig::new("groq", ProviderKind::Groq),
                ProviderConfig::new("google-free", ProviderKind::GoogleFree),
                ProviderConfig { enabled: false, ..ProviderConfig::new("deepl", ProviderKind::Deepl) },
            ],
            translation: Default::default(),
            selection: Default::default(),
            hotkeys: Default::default(),
            popup: Default::default(),
            ocr: Default::default(),
            paused: false,
        }
    }
}

pub fn system_language() -> String {
    sys_locale::get_locale()
        .map(|l| lang::normalize(&l))
        .filter(|l| lang::is_supported(l))
        .unwrap_or_else(|| "en".into())
}

pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(dir: PathBuf) -> Self {
        Self { path: dir.join("settings.json") }
    }

    pub fn load(&self) -> Settings {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Settings::default();
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(s) => migrate(s),
            Err(e) => {
                log::error!("settings.json is invalid ({e}); a backup is kept and defaults are used");
                let _ = std::fs::copy(&self.path, self.path.with_extension("json.bak"));
                Settings::default()
            }
        }
    }

    pub fn save(&self, s: &Settings) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
        std::fs::rename(tmp, &self.path)
    }
}

fn migrate(mut s: Settings) -> Settings {
    if s.version < 2 {
        // v2: solid window background is the default look.
        if s.general.window_effect == WindowEffect::Auto {
            s.general.window_effect = WindowEffect::None;
        }
    }
    if s.version < 3 {
        // v3: popup width is automatic; the setting is the maximum for long texts.
        s.popup.width = 600;
    }
    s.version = SETTINGS_VERSION;
    s
}
