//! Manual check of providers from the command line:
//! `cargo run -p onelang-core --example translate -- "text" [provider]`
//! provider: google_free (default) | groq (needs GROQ_API_KEY) | deepl (needs DEEPL_API_KEY)

use onelang_core::engine::EngineOptions;
use onelang_core::providers::{self, ProviderConfig, ProviderKind};
use onelang_core::{DetectionMode, Engine, LanguagePair, TranslateEvent, TranslateMode, TranslateRequest};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let text = args.next().unwrap_or_else(|| "Hello, how are you doing today?".into());
    let which = args.next().unwrap_or_else(|| "google_free".into());
    let (kind, key) = match which.as_str() {
        "groq" => (ProviderKind::Groq, std::env::var("GROQ_API_KEY").ok()),
        "deepl" => (ProviderKind::Deepl, std::env::var("DEEPL_API_KEY").ok()),
        _ => (ProviderKind::GoogleFree, None),
    };
    let provider = providers::build(ProviderConfig::new("p", kind), key, reqwest::Client::new()).expect("provider");
    let engine = Engine::new(
        vec![provider],
        EngineOptions {
            pair: LanguagePair { primary: "ru".into(), secondary: "en".into() },
            detection: DetectionMode::Local,
            favorite_languages: vec!["ru".into(), "en".into(), "es".into()],
            custom_instructions: None,
            dictionary_for_words: true,
        },
    );
    let req = TranslateRequest { text, mode: TranslateMode::Auto, ..Default::default() };
    let res = engine
        .translate(req, &|e| match e {
            TranslateEvent::Delta { text } => eprint!("{text}"),
            TranslateEvent::Direction { source_lang, target_lang } => eprintln!("[direction {source_lang:?} -> {target_lang}]"),
            TranslateEvent::Started { provider_name, target_lang, .. } => eprintln!("[{provider_name} -> {target_lang:?}]"),
            _ => {}
        })
        .await;
    eprintln!();
    match res {
        Ok(r) => println!("{:#?}", r),
        Err(e) => println!("ERROR: {e}"),
    }
}
