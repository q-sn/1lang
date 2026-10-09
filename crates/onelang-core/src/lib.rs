//! Translation engine of 1lang.
//!
//! This crate knows nothing about Tauri or the OS: it takes text, decides the
//! translation direction, talks to providers and reports results through a
//! callback. Everything here can be unit-tested and reused (CLI, server, …).

pub mod detect;
pub mod engine;
pub mod error;
pub mod lang;
pub mod pricing;
pub mod prompt;
pub mod providers;
pub mod types;

pub use engine::Engine;
pub use error::{Error, Result};
pub use types::*;
