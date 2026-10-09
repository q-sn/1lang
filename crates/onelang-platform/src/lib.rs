//! OS integration. Windows is implemented; other platforms return
//! `Error::Unsupported` until they are added.

pub mod capture;
mod error;
pub mod types;

#[cfg(windows)]
mod win;

pub use error::{Error, Result};
pub use types::*;

#[cfg(windows)]
pub use win::{clipboard, clipboard_watch, hooks, input, ocr, selection, system, window};
