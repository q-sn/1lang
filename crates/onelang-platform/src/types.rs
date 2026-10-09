use serde::{Deserialize, Serialize};
use specta::Type;

/// Rectangle in physical screen pixels.
#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn union(self, o: Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        let r = (self.x + self.width).max(o.x + o.width);
        let b = (self.y + self.height).max(o.y + o.height);
        Rect { x, y, width: r - x, height: b - y }
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// How the selected text is read from other applications.
#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMethod {
    /// Accessibility API only. Never touches the clipboard.
    Uia,
    /// Simulate Ctrl+C and restore the clipboard afterwards.
    Clipboard,
    /// Accessibility API first, clipboard when it returns nothing.
    #[default]
    UiaThenClipboard,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct Selection {
    pub text: String,
    /// Bounds of the selected text (if the app exposes them).
    pub bounds: Option<Rect>,
    /// Bounds of the first line of the selection: where the selection starts.
    pub start_bounds: Option<Rect>,
    /// Bounds of the last line of the selection: where the selection ends.
    pub end_bounds: Option<Rect>,
    /// Surrounding text (paragraphs around the selection).
    pub context: Option<String>,
    pub source: SelectionSource,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    #[default]
    Uia,
    Clipboard,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct ForegroundApp {
    /// Executable file name, lowercase, e.g. "code.exe".
    pub exe: String,
    pub title: String,
    pub pid: u32,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct MonitorInfo {
    pub index: u32,
    /// Physical pixels.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct OcrOutput {
    pub text: String,
    /// BCP-47 tag of the recognizer that produced the result.
    pub language: String,
}
