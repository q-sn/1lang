use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[cfg(windows)]
    #[error("Windows API error: {0}")]
    Windows(#[from] windows::core::Error),
    #[error("Screen capture error: {0}")]
    Capture(String),
    #[error("Clipboard is busy")]
    ClipboardBusy,
    #[error("Timed out")]
    Timeout,
    #[error("OCR language \"{0}\" is not installed")]
    OcrLanguageMissing(String),
    #[error("No OCR languages are installed")]
    NoOcrLanguages,
    #[error("{0}")]
    Other(String),
    #[error("Not supported on this platform")]
    Unsupported,
}

pub type Result<T> = std::result::Result<T, Error>;
