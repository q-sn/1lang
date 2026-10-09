//! Offline language detection (whatlang) tuned for short selections.

use crate::lang;
use whatlang::{Detector, Lang, Script};

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub lang: String,
    /// 0..1
    pub confidence: f64,
}

impl Detection {
    pub fn is_reliable(&self) -> bool {
        self.confidence >= 0.5
    }
}

/// Detect the language of `text`, preferring `preferred` languages when the
/// text is short/ambiguous (a 2-word Latin phrase is far more likely to be in
/// one of the user's languages than in, say, Esperanto).
pub fn detect(text: &str, preferred: &[String]) -> Option<Detection> {
    let sample: String = text.chars().take(2000).collect();
    let letters = sample.chars().filter(|c| c.is_alphabetic()).count();
    if letters == 0 {
        return None;
    }

    let info = whatlang::detect(&sample)?;
    let script = info.script();

    // Scripts that map to exactly one language in practice.
    let by_script = match script {
        Script::Hangul => Some("ko"),
        Script::Hiragana | Script::Katakana => Some("ja"),
        Script::Greek => Some("el"),
        Script::Hebrew => Some("he"),
        Script::Georgian => Some("ka"),
        Script::Armenian => Some("hy"),
        Script::Thai => Some("th"),
        _ => None,
    };
    if let Some(code) = by_script {
        return Some(Detection { lang: code.into(), confidence: 1.0 });
    }

    // Restrict to preferred languages that use the detected script.
    let allow: Vec<Lang> = preferred
        .iter()
        .filter_map(|c| lang::to_iso639_3(c))
        .filter_map(Lang::from_code)
        .filter(|l| script_of(*l) == Some(script))
        .collect();

    let short = letters < 40;
    let general = lang::from_iso639_3(info.lang().code());
    let in_prefs = |code: &str| preferred.iter().any(|p| lang::normalize(p) == code);

    // whatlang is decent on long text in "big" languages; trust it when it is
    // sure, or when it agrees with the user's own languages.
    if !short {
        if let Some(code) = general {
            if in_prefs(code) {
                return Some(Detection { lang: code.into(), confidence: info.confidence().max(0.7) });
            }
            if info.is_reliable() {
                return Some(Detection { lang: code.into(), confidence: info.confidence().max(0.8) });
            }
        }
    }

    // Otherwise choose among preferred languages written in the same script.
    match allow.len() {
        0 => {}
        1 => {
            let code = lang::from_iso639_3(allow[0].code())?;
            return Some(Detection { lang: code.into(), confidence: if short { 0.75 } else { 0.8 } });
        }
        _ => {
            if let Some(i) = Detector::with_allowlist(allow).detect(&sample) {
                if let Some(code) = lang::from_iso639_3(i.lang().code()) {
                    let base = if short { 0.45 } else { 0.6 };
                    return Some(Detection { lang: code.into(), confidence: base + i.confidence() * 0.3 });
                }
            }
        }
    }

    let code = general?;
    let confidence = info.confidence() * if short { 0.5 } else { 1.0 };
    Some(Detection { lang: code.into(), confidence })
}

fn script_of(lang: Lang) -> Option<Script> {
    // Detect the script of a language by detecting a typical word.
    let probe = match lang {
        Lang::Rus | Lang::Ukr | Lang::Bel | Lang::Bul | Lang::Srp | Lang::Mkd => "привет",
        Lang::Ara | Lang::Pes | Lang::Urd => "سلام",
        Lang::Cmn => "你好",
        Lang::Hin | Lang::Mar | Lang::Nep => "नमस्ते",
        _ => "hello",
    };
    whatlang::detect_script(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs() -> Vec<String> {
        vec!["ru".into(), "en".into(), "es".into()]
    }

    #[test]
    fn cyrillic_short_goes_to_russian() {
        assert_eq!(detect("привет", &prefs()).unwrap().lang, "ru");
    }

    #[test]
    fn long_english() {
        let d = detect("The quick brown fox jumps over the lazy dog and keeps running far away", &prefs()).unwrap();
        assert_eq!(d.lang, "en");
        assert!(d.is_reliable());
    }

    #[test]
    fn spanish_sentence() {
        let d = detect("¿Dónde está la biblioteca? Necesito encontrar un libro para mañana.", &prefs()).unwrap();
        assert_eq!(d.lang, "es");
    }

    #[test]
    fn short_english_vs_spanish() {
        assert_eq!(detect("good morning", &prefs()).unwrap().lang, "en");
    }

    #[test]
    fn digits_only() {
        assert!(detect("12345", &prefs()).is_none());
    }
}
