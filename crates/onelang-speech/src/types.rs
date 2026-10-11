use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// Microsoft Edge neural voices, online.
    #[default]
    Edge,
    /// Kokoro-82M, offline.
    Kokoro,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
pub struct Voice {
    pub id: String,
    /// Short display name, e.g. "Sonia".
    pub name: String,
    /// BCP-47, e.g. "en-GB".
    pub locale: String,
    pub gender: Option<String>,
    pub engine: Engine,
}

impl Voice {
    /// ISO 639-1 language of the voice.
    pub fn lang(&self) -> &str {
        lang_of(&self.locale)
    }
}

/// "en-GB" → "en".
pub fn lang_of(locale: &str) -> &str {
    locale.split(['-', '_']).next().unwrap_or(locale)
}

/// "en-GB" → Some("GB").
pub fn region_of(locale: &str) -> Option<&str> {
    locale.split(['-', '_']).nth(1).filter(|r| r.len() == 2)
}

/// Encoded audio.
#[derive(Clone, Debug)]
pub struct Audio {
    pub bytes: Vec<u8>,
    pub format: AudioFormat,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    Mp3,
    Wav,
    Ogg,
    Flac,
}

impl AudioFormat {
    pub const ALL: [AudioFormat; 4] = [AudioFormat::Mp3, AudioFormat::Wav, AudioFormat::Ogg, AudioFormat::Flac];

    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Wav => "wav",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Flac => "flac",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Ogg => "audio/ogg",
            AudioFormat::Flac => "audio/flac",
        }
    }

    pub fn from_extension(ext: &str) -> Option<AudioFormat> {
        match ext.to_ascii_lowercase().as_str() {
            "mp3" => Some(AudioFormat::Mp3),
            "wav" => Some(AudioFormat::Wav),
            "ogg" | "oga" | "opus" => Some(AudioFormat::Ogg),
            "flac" => Some(AudioFormat::Flac),
            _ => None,
        }
    }
}

/// Where a clip came from, shown to the user.
#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClipSource {
    /// A native speaker recording (Wikimedia Commons, via Wiktionary).
    Recording {
        /// Accent label, e.g. "US" or "UK, RP".
        accent: Option<String>,
        /// Commons page of the file (author and license).
        page_url: String,
    },
    Voice { engine: Engine, voice: String },
}

/// A spoken text, stored in the audio cache.
#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct Clip {
    /// Cache file name, e.g. "3fa1….mp3".
    pub file: String,
    pub source: ClipSource,
    /// Playback speed. Synthesized clips already have the requested speed
    /// baked in (1.0); recordings are slowed down by the player.
    pub playback_rate: f32,
}
