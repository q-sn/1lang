//! Kokoro-82M, a small open-weight (Apache 2.0) TTS model, running locally
//! with ONNX Runtime. Text is turned into phonemes by espeak-ng, which runs
//! as a separate process (it is GPL, so it is downloaded with the model
//! instead of being linked into the app).

mod espeak;
mod install;
mod vocab;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ort::session::Session;
use ort::value::Tensor;

use crate::{lang_of, Audio, AudioFormat, Engine, Error, Result, Synthesizer, Voice};

pub use install::{download_size, InstallProgress, InstallStage};

const SAMPLE_RATE: u32 = 24_000;
/// Max tokens per inference (the model's context is 512 incl. padding).
const MAX_TOKENS: usize = 510;
const STYLE_DIM: usize = 256;

/// (id, name, locale, gender)
const VOICES: &[(&str, &str, &str, &str)] = &[
    ("af_heart", "Heart", "en-US", "Female"),
    ("af_bella", "Bella", "en-US", "Female"),
    ("af_nicole", "Nicole", "en-US", "Female"),
    ("af_aoede", "Aoede", "en-US", "Female"),
    ("af_kore", "Kore", "en-US", "Female"),
    ("af_sarah", "Sarah", "en-US", "Female"),
    ("am_michael", "Michael", "en-US", "Male"),
    ("am_fenrir", "Fenrir", "en-US", "Male"),
    ("am_puck", "Puck", "en-US", "Male"),
    ("bf_emma", "Emma", "en-GB", "Female"),
    ("bf_isabella", "Isabella", "en-GB", "Female"),
    ("bm_george", "George", "en-GB", "Male"),
    ("bm_fable", "Fable", "en-GB", "Male"),
    ("ef_dora", "Dora", "es-ES", "Female"),
    ("em_alex", "Alex", "es-ES", "Male"),
    ("ff_siwis", "Siwis", "fr-FR", "Female"),
    ("hf_alpha", "Alpha", "hi-IN", "Female"),
    ("hf_beta", "Beta", "hi-IN", "Female"),
    ("hm_omega", "Omega", "hi-IN", "Male"),
    ("if_sara", "Sara", "it-IT", "Female"),
    ("im_nicola", "Nicola", "it-IT", "Male"),
    ("pf_dora", "Dora", "pt-BR", "Female"),
    ("pm_alex", "Alex", "pt-BR", "Male"),
];

/// Languages Kokoro speaks (with espeak-ng phonemes). Japanese and Chinese
/// voices exist but need their own phonemizers.
pub const LANGUAGES: &[&str] = &["en", "es", "fr", "hi", "it", "pt"];

pub fn voices() -> Vec<Voice> {
    VOICES
        .iter()
        .map(|(id, name, locale, gender)| Voice {
            id: id.to_string(),
            name: name.to_string(),
            locale: locale.to_string(),
            gender: Some(gender.to_string()),
            engine: Engine::Kokoro,
        })
        .collect()
}

pub fn supports(lang: &str) -> bool {
    LANGUAGES.contains(&lang)
}

pub fn default_voice(lang: &str) -> Option<&'static str> {
    Some(match lang {
        "en" => "af_heart",
        "es" => "ef_dora",
        "fr" => "ff_siwis",
        "hi" => "hf_alpha",
        "it" => "if_sara",
        "pt" => "pf_dora",
        _ => return None,
    })
}

pub fn voice_locale(id: &str) -> Option<&'static str> {
    VOICES.iter().find(|v| v.0 == id).map(|v| v.2)
}

pub struct KokoroTts {
    dir: PathBuf,
    session: Arc<Mutex<Option<Session>>>,
}

impl KokoroTts {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, session: Arc::new(Mutex::new(None)) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn model_path(&self) -> PathBuf {
        self.dir.join("model.onnx")
    }

    fn voice_path(&self, id: &str) -> PathBuf {
        self.dir.join("voices").join(format!("{id}.bin"))
    }

    fn espeak(&self) -> espeak::Espeak {
        espeak::Espeak::new(self.dir.join("espeak"))
    }

    pub fn is_installed(&self) -> bool {
        self.model_path().is_file()
            && self.espeak().is_installed()
            && VOICES.iter().all(|v| self.voice_path(v.0).is_file())
    }

    /// Download the model, voices and espeak-ng.
    pub async fn install(&self, http: &reqwest::Client, progress: &(dyn Fn(InstallProgress) + Send + Sync)) -> Result<()> {
        let ids: Vec<&str> = VOICES.iter().map(|v| v.0).collect();
        install::install(http, &self.dir, &ids, progress).await
    }

    pub fn remove(&self) -> Result<()> {
        // Release the model file before deleting it.
        *self.session.lock().unwrap() = None;
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    fn load_style(&self, voice: &str) -> Result<Vec<f32>> {
        let bytes = std::fs::read(self.voice_path(voice)).map_err(|_| Error::NotInstalled)?;
        Ok(bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
    }
}

/// Split tokens into windows of at most `MAX_TOKENS`, preferring sentence
/// ends, then other punctuation, then spaces.
fn batches(tokens: &[i64]) -> Vec<&[i64]> {
    let mut out = vec![];
    let mut rest = tokens;
    while rest.len() > MAX_TOKENS {
        let window = &rest[..MAX_TOKENS];
        let find = |set: &[char]| {
            let ids: Vec<i64> = set.iter().filter_map(|c| vocab::id(*c)).collect();
            window.iter().rposition(|t| ids.contains(t)).filter(|&i| i > MAX_TOKENS / 3)
        };
        let cut = find(&['.', '!', '?'])
            .or_else(|| find(&[',', ';', ':', '—', '…']))
            .or_else(|| find(&[' ']))
            .map(|i| i + 1)
            .unwrap_or(MAX_TOKENS);
        out.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

fn run_model(session: &mut Session, tokens: &[i64], style: &[f32], speed: f32) -> Result<Vec<f32>> {
    let model_err = |e: ort::Error| Error::Model(e.to_string());
    let tokens_input = if session.inputs().iter().any(|i| i.name() == "input_ids") { "input_ids" } else { "tokens" };
    let mut ids = Vec::with_capacity(tokens.len() + 2);
    ids.push(0);
    ids.extend_from_slice(tokens);
    ids.push(0);
    // One style vector per token count.
    let row = tokens.len().min(style.len() / STYLE_DIM - 1);
    let style = style[row * STYLE_DIM..(row + 1) * STYLE_DIM].to_vec();
    let inputs = ort::inputs![
        tokens_input => Tensor::from_array(([1usize, ids.len()], ids)).map_err(model_err)?,
        "style" => Tensor::from_array(([1usize, STYLE_DIM], style)).map_err(model_err)?,
        "speed" => Tensor::from_array(([1usize], vec![speed])).map_err(model_err)?,
    ];
    let outputs = session.run(inputs).map_err(model_err)?;
    let (_, audio) = outputs[0].try_extract_tensor::<f32>().map_err(model_err)?;
    Ok(audio.to_vec())
}

/// 16-bit PCM WAV.
fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

#[async_trait]
impl Synthesizer for KokoroTts {
    fn engine(&self) -> Engine {
        Engine::Kokoro
    }

    fn is_ready(&self) -> bool {
        self.is_installed()
    }

    async fn voices(&self) -> Result<Vec<Voice>> {
        Ok(voices())
    }

    async fn synthesize(&self, text: &str, voice: &str, rate: f32) -> Result<Audio> {
        if !self.is_installed() {
            return Err(Error::NotInstalled);
        }
        let locale = voice_locale(voice).ok_or_else(|| Error::NoVoice(voice.to_string()))?;
        let phonemes = self.espeak().phonemize(text, locale).await?;
        log::debug!("kokoro phonemes: {phonemes}");
        let tokens: Vec<i64> = phonemes.chars().filter_map(vocab::id).collect();
        if tokens.is_empty() {
            return Err(Error::Empty);
        }
        let style = self.load_style(voice)?;
        let session = self.session.clone();
        let model = self.model_path();
        let speed = rate.clamp(0.5, 2.0);
        tokio::task::spawn_blocking(move || {
            let mut guard = session.lock().unwrap();
            if guard.is_none() {
                let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
                let s = Session::builder()
                    .and_then(|b| b.with_intra_threads(threads).map_err(Into::into))
                    .and_then(|mut b| b.commit_from_file(&model))
                    .map_err(|e| Error::Model(e.to_string()))?;
                *guard = Some(s);
            }
            let session = guard.as_mut().unwrap();
            let mut samples = vec![];
            for batch in batches(&tokens) {
                samples.extend(run_model(session, batch, &style, speed)?);
            }
            Ok(Audio { bytes: encode_wav(&samples), format: AudioFormat::Wav })
        })
        .await
        .map_err(|e| Error::Model(e.to_string()))?
    }
}

/// Language of a Kokoro voice id.
pub fn voice_lang(id: &str) -> Option<&'static str> {
    voice_locale(id).map(lang_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batching() {
        let space = vocab::id(' ').unwrap();
        let dot = vocab::id('.').unwrap();
        let mut tokens = vec![50i64; 1200];
        for i in (0..1200).step_by(7) {
            tokens[i] = space;
        }
        tokens[400] = dot;
        let b = batches(&tokens);
        assert!(b.iter().all(|w| w.len() <= MAX_TOKENS));
        assert_eq!(b.iter().map(|w| w.len()).sum::<usize>(), 1200);
        assert_eq!(b[0].len(), 401);
        assert_eq!(batches(&[1, 2, 3]).len(), 1);
    }

    #[test]
    fn voices_are_consistent() {
        for (id, ..) in VOICES {
            assert!(supports(voice_lang(id).unwrap()), "{id}");
        }
        for l in LANGUAGES {
            assert_eq!(voice_lang(default_voice(l).unwrap()), Some(*l));
        }
    }

    #[test]
    fn wav_header() {
        let wav = encode_wav(&[0.0, 0.5, -1.0]);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 6);
    }
}
