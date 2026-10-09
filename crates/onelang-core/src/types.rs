use serde::{Deserialize, Serialize};
use specta::Type;

/// What the user wants to get back.
#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranslateMode {
    /// Dictionary card for a single word (if enabled in options), translation otherwise.
    #[default]
    Auto,
    Translate,
    Dictionary,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Formality {
    #[default]
    Default,
    Formal,
    Informal,
}

/// How the source language is detected when the user did not set it.
#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetectionMode {
    /// Fast offline detector; LLM / provider decides only when it is unsure.
    #[default]
    Local,
    /// Always let the LLM decide (more accurate for short / mixed text).
    Llm,
}

/// The pair used for automatic direction: text in `primary` goes to `secondary`,
/// anything else goes to `primary`.
#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq, Eq)]
pub struct LanguagePair {
    pub primary: String,
    pub secondary: String,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct TranslateRequest {
    pub text: String,
    /// `None` = detect automatically.
    pub source_lang: Option<String>,
    /// `None` = automatic direction using the language pair.
    pub target_lang: Option<String>,
    #[serde(default)]
    pub mode: TranslateMode,
    /// Surrounding text / app info that helps the LLM; never translated itself.
    pub context: Option<String>,
    #[serde(default)]
    pub formality: Formality,
    /// Use only this provider (otherwise the configured fallback chain).
    pub provider_id: Option<String>,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Characters sent to a character-billed provider (DeepL, Google, Microsoft).
    pub characters: u32,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    #[default]
    Text,
    Dictionary,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default, PartialEq)]
pub struct DictionaryExample {
    pub source: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default, PartialEq)]
pub struct DictionarySense {
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub translations: Vec<String>,
    #[serde(default)]
    pub meaning: Option<String>,
    #[serde(default)]
    pub examples: Vec<DictionaryExample>,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default, PartialEq)]
pub struct DictionaryEntry {
    pub word: String,
    #[serde(default)]
    pub lemma: Option<String>,
    #[serde(default)]
    pub transcription: Option<String>,
    #[serde(default)]
    pub senses: Vec<DictionarySense>,
    #[serde(default)]
    pub forms: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct TranslationResult {
    pub text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: Option<String>,
    pub usage: Usage,
    pub cost_usd: Option<f64>,
    pub kind: OutputKind,
    pub dictionary: Option<DictionaryEntry>,
    pub cached: bool,
    pub elapsed_ms: u32,
}

/// Streamed to the UI while a translation is running.
#[derive(Serialize, Deserialize, Type, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranslateEvent {
    Started {
        provider_id: String,
        provider_name: String,
        model: Option<String>,
        source_lang: Option<String>,
        target_lang: Option<String>,
        kind: OutputKind,
    },
    /// Direction became known while streaming (LLM decided it).
    Direction { source_lang: Option<String>, target_lang: String },
    Delta { text: String },
    /// The provider failed in the middle; text shown so far must be dropped.
    Reset { reason: String },
    Done { result: Box<TranslationResult> },
    Error { message: String },
}
