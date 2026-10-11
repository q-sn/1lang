use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cache::{self, AudioCache};
use crate::edge::{self, EdgeTts};
use crate::kokoro::{self, InstallProgress, KokoroTts};
use crate::recordings::Recordings;
use crate::{lang_of, region_of, Clip, ClipSource, Engine, Error, Result, Synthesizer, Voice};

#[derive(Clone, Debug, Default)]
pub struct SpeakOptions {
    pub engine: Engine,
    /// Voices chosen by the user for the language (None = default).
    pub edge_voice: Option<String>,
    pub kokoro_voice: Option<String>,
    /// Speed multiplier, 1.0 = normal.
    pub rate: f32,
    /// Prefer native speaker recordings for single words.
    pub recordings: bool,
}

/// Picks the best audio source for a request and caches the result:
/// recording (single words) → selected engine → Edge as the fallback.
pub struct Speaker {
    http: reqwest::Client,
    edge: EdgeTts,
    kokoro: KokoroTts,
    recordings: Recordings,
    cache: AudioCache,
    installing: AtomicBool,
}

/// The text without surrounding punctuation, if it is a single word.
fn single_word<'a>(text: &'a str, lang: &str) -> Option<&'a str> {
    let word = text.trim_matches(|c: char| !c.is_alphanumeric());
    let max = if matches!(lang, "zh" | "ja" | "th") { 4 } else { 40 };
    (!word.is_empty() && !word.chars().any(char::is_whitespace) && word.chars().count() <= max).then_some(word)
}

impl Speaker {
    pub fn new(http: reqwest::Client, kokoro_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            edge: EdgeTts::new(http.clone()),
            kokoro: KokoroTts::new(kokoro_dir),
            recordings: Recordings::new(http.clone()),
            cache: AudioCache::new(cache_dir),
            http,
            installing: AtomicBool::new(false),
        }
    }

    pub fn cache(&self) -> &AudioCache {
        &self.cache
    }

    pub fn kokoro_installed(&self) -> bool {
        self.kokoro.is_installed()
    }

    pub fn kokoro_installing(&self) -> bool {
        self.installing.load(Ordering::Relaxed)
    }

    pub async fn install_kokoro(&self, progress: &(dyn Fn(InstallProgress) + Send + Sync)) -> Result<()> {
        if self.installing.swap(true, Ordering::SeqCst) {
            return Err(Error::Service("already downloading".into()));
        }
        let r = self.kokoro.install(&self.http, progress).await;
        self.installing.store(false, Ordering::SeqCst);
        r
    }

    pub fn remove_kokoro(&self) -> Result<()> {
        self.kokoro.remove()
    }

    pub async fn voices(&self, engine: Engine) -> Result<Vec<Voice>> {
        match engine {
            Engine::Edge => self.edge.voices().await,
            Engine::Kokoro => self.kokoro.voices().await,
        }
    }

    async fn edge_voice(&self, lang: &str, chosen: Option<&str>) -> Result<String> {
        if let Some(v) = chosen.filter(|v| edge::voice_lang(v) == lang) {
            return Ok(v.to_string());
        }
        let list = self.edge.voices().await.ok();
        edge::default_voice(lang, list.as_deref()).ok_or_else(|| Error::NoVoice(lang.to_string()))
    }

    fn kokoro_voice(lang: &str, chosen: Option<&str>) -> Option<String> {
        chosen
            .filter(|v| kokoro::voice_lang(v) == Some(lang))
            .or_else(|| kokoro::default_voice(lang))
            .map(str::to_string)
    }

    async fn synthesize(&self, engine: &dyn Synthesizer, text: &str, voice: &str, rate: f32) -> Result<Clip> {
        let source = ClipSource::Voice { engine: engine.engine(), voice: voice.to_string() };
        let key = cache::key(&["voice", &format!("{:?}", engine.engine()), voice, &format!("{rate:.2}"), text]);
        let file = match self.cache.find(&key) {
            Some(f) => f,
            None => self.cache.put(&key, &engine.synthesize(text, voice, rate).await?)?,
        };
        Ok(Clip { file, source, playback_rate: 1.0 })
    }

    async fn recording(&self, word: &str, lang: &str, region: Option<&str>, rate: f32) -> Result<Option<Clip>> {
        let Some(rec) = self.recordings.find(word, lang, region).await? else {
            return Ok(None);
        };
        let key = cache::key(&["recording", &rec.file]);
        let file = match self.cache.find(&key) {
            Some(f) => f,
            None => self.cache.put(&key, &self.recordings.download(&rec).await?)?,
        };
        let source = ClipSource::Recording { accent: rec.accent.clone(), page_url: rec.page_url() };
        Ok(Some(Clip { file, source, playback_rate: rate }))
    }

    /// Speak `text` in `lang` (ISO 639-1 or a locale).
    pub async fn speak(&self, text: &str, lang: &str, opts: &SpeakOptions) -> Result<Clip> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Empty);
        }
        let lang = lang_of(lang).to_ascii_lowercase();
        let rate = if opts.rate > 0.0 { opts.rate } else { 1.0 };
        let kokoro_voice = (opts.engine == Engine::Kokoro && self.kokoro.is_ready())
            .then(|| Self::kokoro_voice(&lang, opts.kokoro_voice.as_deref()))
            .flatten();

        if opts.recordings {
            if let Some(word) = single_word(text, &lang) {
                // Prefer the accent of the voice that would speak otherwise.
                let locale = match &kokoro_voice {
                    Some(v) => kokoro::voice_locale(v).map(str::to_string),
                    None => self.edge_voice(&lang, opts.edge_voice.as_deref()).await.ok().map(|v| edge::voice_locale(&v)),
                };
                match self.recording(word, &lang, locale.as_deref().and_then(region_of), rate).await {
                    Ok(Some(clip)) => return Ok(clip),
                    Ok(None) => {}
                    Err(e) => log::info!("recording of {word:?}: {e}"),
                }
            }
        }

        if let Some(voice) = &kokoro_voice {
            match self.synthesize(&self.kokoro, text, voice, rate).await {
                Ok(clip) => return Ok(clip),
                Err(e) => log::warn!("kokoro failed, using Edge: {e}"),
            }
        }
        let edge = match self.edge_voice(&lang, opts.edge_voice.as_deref()).await {
            Ok(voice) => self.synthesize(&self.edge, text, &voice, rate).await,
            Err(e) => Err(e),
        };
        // The online service is unofficial and may fail: an installed offline
        // voice is better than nothing.
        match edge {
            Err(e) if kokoro_voice.is_none() && self.kokoro.is_ready() => {
                let Some(voice) = Self::kokoro_voice(&lang, opts.kokoro_voice.as_deref()) else {
                    return Err(e);
                };
                log::warn!("edge failed, using Kokoro: {e}");
                self.synthesize(&self.kokoro, text, &voice, rate).await.map_err(|_| e)
            }
            r => r,
        }
    }

    /// Speak with a specific voice (voice previews in settings).
    pub async fn speak_with(&self, text: &str, engine: Engine, voice: &str, rate: f32) -> Result<Clip> {
        match engine {
            Engine::Edge => self.synthesize(&self.edge, text, voice, rate).await,
            Engine::Kokoro => self.synthesize(&self.kokoro, text, voice, rate).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_words() {
        assert_eq!(single_word("  Water! ", "en"), Some("Water"));
        assert_eq!(single_word("«hola»", "es"), Some("hola"));
        assert_eq!(single_word("well-known", "en"), Some("well-known"));
        assert_eq!(single_word("two words", "en"), None);
        assert_eq!(single_word("...", "en"), None);
        assert_eq!(single_word("你好", "zh"), Some("你好"));
        assert_eq!(single_word("我们今天去公园散步", "zh"), None);
    }
}
