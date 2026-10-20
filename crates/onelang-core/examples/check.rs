//! Manual check of the writing check from the command line:
//! `cargo run -p onelang-core --example check -- "text" [errors|natural|rewrite]`
//! Needs GROQ_API_KEY; explanations are in Russian.

use onelang_core::engine::EngineOptions;
use onelang_core::providers::{self, ProviderConfig, ProviderKind};
use onelang_core::{CheckLevel, CheckRequest, DetectionMode, Engine, LanguagePair};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let text = args
        .next()
        .unwrap_or_else(|| "Hi team, I want to inform that the release will be delay on two days.".into());
    let level = match args.next().as_deref() {
        Some("errors") => CheckLevel::Errors,
        Some("rewrite") => CheckLevel::Rewrite,
        _ => CheckLevel::Natural,
    };
    let key = std::env::var("GROQ_API_KEY").ok();
    let provider =
        providers::build(ProviderConfig::new("groq", ProviderKind::Groq), key, reqwest::Client::new()).expect("provider");
    let engine = Engine::new(
        vec![provider],
        EngineOptions {
            pair: LanguagePair { primary: "ru".into(), secondary: "en".into() },
            detection: DetectionMode::Local,
            favorite_languages: vec!["ru".into(), "en".into()],
            custom_instructions: None,
            dictionary_for_words: false,
        },
    );
    match engine.check(CheckRequest { text, level, ..Default::default() }).await {
        Ok(r) => {
            println!("{} ({:?}, {} ms)\n", r.corrected, r.lang, r.elapsed_ms);
            for s in r.segments.iter().filter(|s| s.is_edit()) {
                println!("  edit: {:?} -> {:?}", s.original, s.corrected);
            }
            println!();
            for i in &r.issues {
                println!("  [{:?}/{:?}] {} -> {} {:?}\n    {}", i.severity, i.kind, i.original, i.correction, i.segments, i.explanation);
            }
        }
        Err(e) => eprintln!("error: {e}"),
    }
}
