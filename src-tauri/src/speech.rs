//! Glue between the UI and onelang-speech: speech options from settings and
//! the `speech` protocol that serves cached clips to the audio player.

use onelang_speech::{SpeakOptions, Speaker};
use tauri::http::Response;

use crate::settings::Settings;

/// Clips larger than this in total are pruned on start.
const CACHE_LIMIT: u64 = 200 * 1024 * 1024;

pub fn options(settings: &Settings, lang: &str, slow: bool) -> SpeakOptions {
    let s = &settings.speech;
    let lang = onelang_speech::lang_of(lang).to_ascii_lowercase();
    SpeakOptions {
        engine: s.engine,
        edge_voice: s.edge_voices.get(&lang).cloned(),
        kokoro_voice: s.kokoro_voices.get(&lang).cloned(),
        rate: if slow { s.slow_rate } else { s.rate } as f32 / 100.0,
        recordings: s.word_recordings,
    }
}

/// URL of a cached clip for `<audio>`.
pub fn clip_url(file: &str) -> String {
    format!("http://speech.localhost/{file}")
}

/// Response of the `speech` protocol: http://speech.localhost/<file>
pub fn serve(speaker: &Speaker, path: &str) -> Response<Vec<u8>> {
    let name = path.trim_start_matches('/');
    let resp = match speaker.cache().read(name) {
        Some((bytes, format)) => Response::builder()
            .header("Content-Type", format.mime())
            .header("Cache-Control", "max-age=31536000, immutable")
            .body(bytes),
        None => Response::builder().status(404).body(Vec::new()),
    };
    resp.unwrap()
}

pub fn prune_cache(speaker: &Speaker) {
    speaker.cache().prune(CACHE_LIMIT);
}
