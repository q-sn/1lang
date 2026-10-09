//! Translation providers.
//!
//! LLM providers that speak the OpenAI chat-completions protocol (Groq, OpenAI,
//! OpenRouter, Gemini, Anthropic, Ollama, LM Studio, …) share one adapter;
//! machine-translation APIs have their own small adapters.

mod deepl;
mod google;
mod microsoft;
mod openai_compat;

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::prompt::Job;
use crate::types::{DictionaryEntry, Usage};
use crate::{Error, Result};

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Groq,
    OpenAi,
    OpenRouter,
    Gemini,
    Anthropic,
    Ollama,
    LmStudio,
    /// Any other OpenAI-compatible endpoint.
    OpenAiCompatible,
    Deepl,
    GoogleCloud,
    /// Unofficial free Google endpoint, no key needed.
    GoogleFree,
    Microsoft,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 12] = [
        ProviderKind::Groq,
        ProviderKind::OpenAi,
        ProviderKind::OpenRouter,
        ProviderKind::Gemini,
        ProviderKind::Anthropic,
        ProviderKind::Ollama,
        ProviderKind::LmStudio,
        ProviderKind::OpenAiCompatible,
        ProviderKind::Deepl,
        ProviderKind::GoogleCloud,
        ProviderKind::GoogleFree,
        ProviderKind::Microsoft,
    ];

    pub fn is_llm(self) -> bool {
        !matches!(
            self,
            ProviderKind::Deepl | ProviderKind::GoogleCloud | ProviderKind::GoogleFree | ProviderKind::Microsoft
        )
    }

    pub fn needs_key(self) -> bool {
        !matches!(
            self,
            ProviderKind::Ollama | ProviderKind::LmStudio | ProviderKind::GoogleFree | ProviderKind::OpenAiCompatible
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Groq => "Groq",
            ProviderKind::OpenAi => "OpenAI",
            ProviderKind::OpenRouter => "OpenRouter",
            ProviderKind::Gemini => "Google Gemini",
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::Ollama => "Ollama",
            ProviderKind::LmStudio => "LM Studio",
            ProviderKind::OpenAiCompatible => "OpenAI-compatible",
            ProviderKind::Deepl => "DeepL",
            ProviderKind::GoogleCloud => "Google Cloud Translation",
            ProviderKind::GoogleFree => "Google Translate (free)",
            ProviderKind::Microsoft => "Microsoft Translator",
        }
    }

    pub fn default_base_url(self) -> Option<&'static str> {
        Some(match self {
            ProviderKind::Groq => "https://api.groq.com/openai/v1",
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::OpenRouter => "https://openrouter.ai/api/v1",
            ProviderKind::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            ProviderKind::Anthropic => "https://api.anthropic.com/v1",
            ProviderKind::Ollama => "http://localhost:11434/v1",
            ProviderKind::LmStudio => "http://localhost:1234/v1",
            _ => return None,
        })
    }

    pub fn default_model(self) -> Option<&'static str> {
        Some(match self {
            ProviderKind::Groq => "openai/gpt-oss-120b",
            ProviderKind::OpenAi => "gpt-4.1-mini",
            ProviderKind::OpenRouter => "openai/gpt-4.1-mini",
            ProviderKind::Gemini => "gemini-2.5-flash",
            ProviderKind::Anthropic => "claude-haiku-5-5",
            ProviderKind::Ollama => "qwen3:8b",
            _ => return None,
        })
    }

    /// Models worth choosing for translation, best first.
    pub fn recommended_models(self) -> Vec<RecommendedModel> {
        use ModelTier::*;
        let list: &[(&str, ModelTier)] = match self {
            ProviderKind::Groq => &[
                ("openai/gpt-oss-120b", Recommended),
                ("openai/gpt-oss-20b", Fast),
                ("llama-3.3-70b-versatile", Quality),
            ],
            ProviderKind::OpenAi => &[("gpt-4.1-mini", Recommended), ("gpt-4.1-nano", Fast), ("gpt-4.1", Quality)],
            ProviderKind::OpenRouter => &[
                ("openai/gpt-4.1-mini", Recommended),
                ("google/gemini-2.5-flash-lite", Fast),
                ("anthropic/claude-sonnet-5-5", Quality),
            ],
            ProviderKind::Gemini => &[
                ("gemini-2.5-flash", Recommended),
                ("gemini-2.5-flash-lite", Fast),
                ("gemini-2.5-pro", Quality),
            ],
            ProviderKind::Anthropic => &[("claude-haiku-5-5", Recommended), ("claude-sonnet-5-5", Quality)],
            ProviderKind::Ollama => &[("qwen3:8b", Recommended), ("gemma3:4b", Fast), ("gemma3:27b", Quality)],
            _ => &[],
        };
        list.iter().map(|(id, tier)| RecommendedModel { id: id.to_string(), tier: *tier }).collect()
    }

    /// Price per 1M characters for character-billed APIs (paid tier).
    pub fn default_char_price(self) -> Option<f64> {
        match self {
            ProviderKind::Deepl => Some(25.0),
            ProviderKind::GoogleCloud => Some(20.0),
            ProviderKind::Microsoft => Some(10.0),
            _ => None,
        }
    }
}

/// Why a model is suggested; shown as a label in the model picker.
#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    /// Best balance of quality, speed and price for translation.
    Recommended,
    Fast,
    Quality,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
pub struct RecommendedModel {
    pub id: String,
    pub tier: ModelTier,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Azure region for Microsoft Translator.
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Extra JSON merged into the chat-completions body (LLM providers).
    #[serde(default)]
    pub extra_body: Option<String>,
    /// LLM: $ per 1M input tokens. Character APIs: $ per 1M characters.
    #[serde(default)]
    pub price_input: Option<f64>,
    /// LLM: $ per 1M output tokens.
    #[serde(default)]
    pub price_output: Option<f64>,
}

impl ProviderConfig {
    pub fn new(id: impl Into<String>, kind: ProviderKind) -> Self {
        Self {
            id: id.into(),
            kind,
            name: kind.label().to_string(),
            enabled: true,
            base_url: None,
            model: kind.default_model().map(Into::into),
            region: None,
            temperature: None,
            extra_body: None,
            price_input: None,
            price_output: None,
        }
    }

    pub fn base_url(&self) -> Option<String> {
        self.base_url
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.kind.default_base_url().map(Into::into))
            .map(|s| s.trim_end_matches('/').to_string())
    }

    pub fn model(&self) -> Option<String> {
        self.model
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.kind.default_model().map(Into::into))
    }

    /// Cost of a request in USD, if prices are known.
    pub fn cost(&self, usage: &Usage) -> Option<f64> {
        if self.kind.is_llm() {
            let (di, do_) = self.model().and_then(|m| crate::pricing::default_model_price(&m)).unzip();
            let pi = self.price_input.or(di)?;
            let po = self.price_output.or(do_)?;
            Some(crate::pricing::token_cost(usage.input_tokens, usage.output_tokens, pi, po))
        } else {
            let p = self.price_input.or(self.kind.default_char_price())?;
            Some(crate::pricing::char_cost(usage.characters, p))
        }
    }
}

/// Piece of a streamed answer.
pub enum StreamPiece {
    Text(String),
    Direction { source: String, target: String },
}

pub type Sink<'a> = &'a (dyn Fn(StreamPiece) + Send + Sync);

#[derive(Debug, Clone, Default)]
pub struct ProviderOutput {
    pub text: String,
    pub source: Option<String>,
    pub target: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Default)]
pub struct DictionaryOutput {
    pub entry: DictionaryEntry,
    pub source: Option<String>,
    pub target: Option<String>,
    pub usage: Usage,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn config(&self) -> &ProviderConfig;

    fn is_llm(&self) -> bool {
        self.config().kind.is_llm()
    }

    /// Translate `job`. Machine-translation providers always receive `Target::Fixed`.
    async fn translate(&self, job: &Job, sink: Sink<'_>) -> Result<ProviderOutput>;

    async fn dictionary(&self, _job: &Job) -> Result<DictionaryOutput> {
        Err(Error::Unsupported)
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

pub fn build(config: ProviderConfig, api_key: Option<String>, http: reqwest::Client) -> Result<Arc<dyn Provider>> {
    let key = api_key.filter(|k| !k.trim().is_empty());
    if config.kind.needs_key() && key.is_none() {
        return Err(Error::MissingApiKey(config.name.clone()));
    }
    Ok(match config.kind {
        k if k.is_llm() => Arc::new(openai_compat::OpenAiCompat::new(config, key, http)?),
        ProviderKind::Deepl => Arc::new(deepl::Deepl::new(config, key.unwrap_or_default(), http)),
        ProviderKind::GoogleCloud => Arc::new(google::GoogleCloud::new(config, key.unwrap_or_default(), http)),
        ProviderKind::GoogleFree => Arc::new(google::GoogleFree::new(config, http)),
        ProviderKind::Microsoft => Arc::new(microsoft::Microsoft::new(config, key.unwrap_or_default(), http)),
        _ => unreachable!(),
    })
}

/// Text sent to a character-billed API.
pub(crate) fn char_count(text: &str) -> u32 {
    text.chars().count() as u32
}

pub(crate) fn target_code(job: &Job) -> Result<&str> {
    match &job.target {
        crate::prompt::Target::Fixed(t) => Ok(t),
        crate::prompt::Target::Swap { .. } => Err(Error::BadResponse("machine translation needs a fixed target".into())),
    }
}
