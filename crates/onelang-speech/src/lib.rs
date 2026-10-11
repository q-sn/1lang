//! Text to speech of 1lang.
//!
//! Like `onelang-core`, this crate knows nothing about Tauri: it turns text
//! into audio files and leaves playback to the caller.
//!
//! - [`edge`]: Microsoft Edge "Read aloud" neural voices (online, free, no key)
//! - [`kokoro`]: Kokoro-82M running locally (offline, downloaded on demand)
//! - [`recordings`]: native speaker recordings of single words from Wiktionary
//! - [`Speaker`]: picks the best source for a request and caches the audio

pub mod cache;
pub mod edge;
pub mod error;
pub mod kokoro;
pub mod recordings;
mod speaker;
pub mod types;

pub use error::{Error, Result};
pub use speaker::{Speaker, SpeakOptions};
pub use types::*;

use async_trait::async_trait;

/// A speech synthesizer. New engines (cloud APIs with a key, other local
/// models) only need to implement this and be wired into [`Speaker`].
#[async_trait]
pub trait Synthesizer: Send + Sync {
    fn engine(&self) -> Engine;

    /// Whether the engine can be used right now (e.g. its model is installed).
    fn is_ready(&self) -> bool {
        true
    }

    async fn voices(&self) -> Result<Vec<Voice>>;

    /// `rate` is a speed multiplier, 1.0 = normal.
    async fn synthesize(&self, text: &str, voice: &str, rate: f32) -> Result<Audio>;
}
