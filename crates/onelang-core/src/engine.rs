//! Orchestrates a translation: mode, direction, provider fallback chain, cost.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::detect;
use crate::lang::normalize;
use crate::prompt::{Job, Target};
use crate::providers::{Provider, StreamPiece};
use crate::types::*;
use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct EngineOptions {
    pub pair: LanguagePair,
    pub detection: DetectionMode,
    /// Languages the user works with; used to disambiguate short texts.
    pub favorite_languages: Vec<String>,
    pub custom_instructions: Option<String>,
    pub dictionary_for_words: bool,
}

pub struct Engine {
    providers: Vec<Arc<dyn Provider>>,
    options: EngineOptions,
}

pub type Emit<'a> = &'a (dyn Fn(TranslateEvent) + Send + Sync);

/// A word or a two-word phrase without sentence punctuation.
pub fn is_dictionary_candidate(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty()
        && t.chars().count() <= 40
        && !t.contains('\n')
        && t.split_whitespace().count() <= 2
        && t.chars().any(char::is_alphabetic)
        && !t.chars().any(|c| matches!(c, '.' | '!' | '?' | ';' | ':' | '/' | '@' | '='))
}

impl Engine {
    pub fn new(providers: Vec<Arc<dyn Provider>>, options: EngineOptions) -> Self {
        Self { providers, options }
    }

    fn swap_target(&self, source: &str) -> String {
        let pair = &self.options.pair;
        if normalize(source) == normalize(&pair.primary) {
            pair.secondary.clone()
        } else {
            pair.primary.clone()
        }
    }

    fn chain(&self, provider_id: Option<&str>) -> Result<Vec<Arc<dyn Provider>>> {
        let chain: Vec<_> = match provider_id {
            Some(id) => self.providers.iter().filter(|p| p.config().id == id).cloned().collect(),
            None => self.providers.clone(),
        };
        if chain.is_empty() {
            return Err(Error::NoProviders);
        }
        Ok(chain)
    }

    pub async fn translate(&self, req: TranslateRequest, emit: Emit<'_>) -> Result<TranslationResult> {
        let started = Instant::now();
        let text = req.text.trim_matches(|c: char| c == '\u{feff}' || c.is_whitespace() && c != '\n');
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::EmptyText);
        }
        let chain = self.chain(req.provider_id.as_deref())?;
        let has_llm = chain.iter().any(|p| p.is_llm());

        let mode = match req.mode {
            TranslateMode::Auto if self.options.dictionary_for_words && is_dictionary_candidate(text) => {
                TranslateMode::Dictionary
            }
            TranslateMode::Auto => TranslateMode::Translate,
            m => m,
        };
        let mode = if mode == TranslateMode::Dictionary && !has_llm { TranslateMode::Translate } else { mode };

        // Direction.
        let explicit_source = req.source_lang.as_deref().map(normalize).filter(|s| s != "auto");
        let mut hints = self.options.favorite_languages.clone();
        hints.extend([self.options.pair.primary.clone(), self.options.pair.secondary.clone()]);
        let guessed_source = explicit_source.clone().or_else(|| {
            if self.options.detection == DetectionMode::Local {
                detect::detect(text, &hints).filter(|d| d.is_reliable()).map(|d| d.lang)
            } else {
                None
            }
        });
        let explicit_target = req.target_lang.as_deref().map(normalize).filter(|s| s != "auto");
        let auto_target = explicit_target.is_none();

        let mut last_err = Error::NoProviders;
        for provider in chain {
            if mode == TranslateMode::Dictionary && !provider.is_llm() {
                continue;
            }
            let cfg = provider.config().clone();
            let target = match (&explicit_target, &guessed_source) {
                (Some(t), _) => Target::Fixed(t.clone()),
                (None, Some(s)) => Target::Fixed(self.swap_target(s)),
                (None, None) if provider.is_llm() => Target::Swap {
                    primary: self.options.pair.primary.clone(),
                    secondary: self.options.pair.secondary.clone(),
                },
                (None, None) => Target::Fixed(self.options.pair.primary.clone()),
            };
            let mut job = Job {
                text: text.to_string(),
                source: explicit_source.clone(),
                target: target.clone(),
                context: req.context.clone(),
                formality: req.formality,
                custom_instructions: self.options.custom_instructions.clone(),
            };
            let kind = if mode == TranslateMode::Dictionary { OutputKind::Dictionary } else { OutputKind::Text };
            emit(TranslateEvent::Started {
                provider_id: cfg.id.clone(),
                provider_name: cfg.name.clone(),
                model: if cfg.kind.is_llm() { cfg.model() } else { None },
                source_lang: explicit_source.clone().or(guessed_source.clone()),
                target_lang: match &target {
                    Target::Fixed(t) => Some(t.clone()),
                    Target::Swap { .. } => None,
                },
                kind,
            });

            let streamed = Arc::new(AtomicBool::new(false));
            let attempt = if mode == TranslateMode::Dictionary {
                provider.dictionary(&job).await.map(|d| {
                    let text = d
                        .entry
                        .senses
                        .iter()
                        .flat_map(|s| s.translations.first())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    (text, d.source, d.target, d.usage, Some(d.entry))
                })
            } else {
                let sink_streamed = streamed.clone();
                let sink = move |piece: StreamPiece| match piece {
                    StreamPiece::Text(t) => {
                        sink_streamed.store(true, Ordering::Relaxed);
                        emit(TranslateEvent::Delta { text: t });
                    }
                    StreamPiece::Direction { source, target } => {
                        emit(TranslateEvent::Direction { source_lang: Some(source), target_lang: target })
                    }
                };
                let mut res = provider.translate(&job, &sink).await;

                // Machine translation with automatic direction: if the text was
                // already in the primary language, translate it to the secondary one.
                if let Ok(out) = &res {
                    let same = out.source.as_deref().map(normalize) == out.target.as_deref().map(normalize);
                    if auto_target && guessed_source.is_none() && !provider.is_llm() && same {
                        emit(TranslateEvent::Reset { reason: "direction".into() });
                        job.source = out.source.clone();
                        job.target = Target::Fixed(self.options.pair.secondary.clone());
                        emit(TranslateEvent::Direction {
                            source_lang: out.source.clone(),
                            target_lang: self.options.pair.secondary.clone(),
                        });
                        let first_usage = out.usage.clone();
                        res = provider.translate(&job, &sink).await.map(|mut o| {
                            o.usage.characters += first_usage.characters;
                            o
                        });
                    }
                }
                res.map(|o| (o.text, o.source, o.target, o.usage, None))
            };

            match attempt {
                Ok((out_text, source, target_out, usage, dictionary)) => {
                    let target_lang = target_out
                        .or(match &job.target {
                            Target::Fixed(t) => Some(t.clone()),
                            Target::Swap { primary, .. } => Some(primary.clone()),
                        })
                        .unwrap_or_default();
                    let result = TranslationResult {
                        text: out_text,
                        source_lang: source.or(guessed_source.clone()),
                        target_lang,
                        provider_id: cfg.id.clone(),
                        provider_name: cfg.name.clone(),
                        model: if cfg.kind.is_llm() { cfg.model() } else { None },
                        cost_usd: cfg.cost(&usage),
                        usage,
                        kind,
                        dictionary,
                        cached: false,
                        elapsed_ms: started.elapsed().as_millis() as u32,
                    };
                    return Ok(result);
                }
                Err(e) => {
                    log::warn!("provider {} failed: {e}", cfg.name);
                    if streamed.load(Ordering::Relaxed) {
                        emit(TranslateEvent::Reset { reason: e.to_string() });
                    }
                    if !e.is_retryable_with_other_provider() {
                        return Err(e);
                    }
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderConfig, ProviderKind, ProviderOutput, Sink};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct Echo {
        cfg: ProviderConfig,
        fail: bool,
        seen: Mutex<Vec<Target>>,
    }

    #[async_trait]
    impl Provider for Echo {
        fn config(&self) -> &ProviderConfig {
            &self.cfg
        }
        async fn translate(&self, job: &Job, sink: Sink<'_>) -> Result<ProviderOutput> {
            self.seen.lock().unwrap().push(job.target.clone());
            if self.fail {
                return Err(Error::BadResponse("boom".into()));
            }
            sink(StreamPiece::Text("ok".into()));
            Ok(ProviderOutput { text: "ok".into(), ..Default::default() })
        }
    }

    fn opts() -> EngineOptions {
        EngineOptions {
            pair: LanguagePair { primary: "ru".into(), secondary: "en".into() },
            detection: DetectionMode::Local,
            favorite_languages: vec!["ru".into(), "en".into(), "es".into()],
            custom_instructions: None,
            dictionary_for_words: false,
        }
    }

    fn echo(id: &str, fail: bool) -> Arc<Echo> {
        Arc::new(Echo { cfg: ProviderConfig::new(id, ProviderKind::Groq), fail, seen: Mutex::new(vec![]) })
    }

    #[tokio::test]
    async fn russian_goes_to_secondary_and_fallback_works() {
        let a = echo("a", true);
        let b = echo("b", false);
        let engine = Engine::new(vec![a.clone(), b.clone()], opts());
        let res = engine
            .translate(
                TranslateRequest { text: "Привет, как твои дела сегодня?".into(), ..Default::default() },
                &|_| {},
            )
            .await
            .unwrap();
        assert_eq!(res.provider_id, "b");
        assert_eq!(res.target_lang, "en");
        assert_eq!(b.seen.lock().unwrap()[0], Target::Fixed("en".into()));
    }

    #[test]
    fn dictionary_candidates() {
        assert!(is_dictionary_candidate("serendipity"));
        assert!(is_dictionary_candidate("look up"));
        assert!(!is_dictionary_candidate("This is a sentence."));
        assert!(!is_dictionary_candidate("https://x.com"));
    }
}
