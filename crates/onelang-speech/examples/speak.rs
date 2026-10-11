//! Try the speech engines:
//!
//! ```text
//! cargo run -p onelang-speech --example speak -- "Hello there" en
//! cargo run -p onelang-speech --example speak -- water en recordings
//! cargo run -p onelang-speech --example speak -- "Hola, ¿qué tal?" es kokoro
//! ```
//!
//! Audio is written to the system temp folder; `kokoro` downloads the model
//! (~340 MB) into it on first use.

use onelang_speech::{Engine, SpeakOptions, Speaker};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text = args.first().cloned().unwrap_or_else(|| "The quick brown fox jumps over the lazy dog.".into());
    let lang = args.get(1).cloned().unwrap_or_else(|| "en".into());
    let mode = args.get(2).map(String::as_str).unwrap_or("edge");

    let tmp = std::env::temp_dir().join("onelang-speech");
    let speaker = Speaker::new(reqwest::Client::new(), tmp.join("kokoro"), tmp.join("cache"));
    if mode == "kokoro" && !speaker.kokoro_installed() {
        speaker
            .install_kokoro(&|p| eprint!("\r{:?} {:.0}%   ", p.stage, p.done / p.total * 100.0))
            .await
            .expect("install");
        eprintln!();
    }
    let opts = SpeakOptions {
        engine: if mode == "kokoro" { Engine::Kokoro } else { Engine::Edge },
        rate: 1.0,
        recordings: mode == "recordings",
        ..Default::default()
    };
    let started = std::time::Instant::now();
    match speaker.speak(&text, &lang, &opts).await {
        Ok(clip) => println!(
            "{:?} in {} ms → {}",
            clip.source,
            started.elapsed().as_millis(),
            tmp.join("cache").join(&clip.file).display()
        ),
        Err(e) => eprintln!("error: {e}"),
    }
}
