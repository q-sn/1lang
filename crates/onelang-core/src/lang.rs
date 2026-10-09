//! Supported languages. Codes are ISO 639-1; display names are produced on the
//! frontend with `Intl.DisplayNames`, so no name tables are kept here.

/// (ISO 639-1, ISO 639-3 used by whatlang, English name for prompts)
const LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "eng", "English"),
    ("ru", "rus", "Russian"),
    ("es", "spa", "Spanish"),
    ("de", "deu", "German"),
    ("fr", "fra", "French"),
    ("it", "ita", "Italian"),
    ("pt", "por", "Portuguese"),
    ("uk", "ukr", "Ukrainian"),
    ("be", "bel", "Belarusian"),
    ("pl", "pol", "Polish"),
    ("cs", "ces", "Czech"),
    ("nl", "nld", "Dutch"),
    ("sv", "swe", "Swedish"),
    ("da", "dan", "Danish"),
    ("nb", "nob", "Norwegian"),
    ("fi", "fin", "Finnish"),
    ("el", "ell", "Greek"),
    ("hu", "hun", "Hungarian"),
    ("ro", "ron", "Romanian"),
    ("bg", "bul", "Bulgarian"),
    ("sr", "srp", "Serbian"),
    ("hr", "hrv", "Croatian"),
    ("tr", "tur", "Turkish"),
    ("kk", "kaz", "Kazakh"),
    ("uz", "uzb", "Uzbek"),
    ("ka", "kat", "Georgian"),
    ("hy", "hye", "Armenian"),
    ("he", "heb", "Hebrew"),
    ("ar", "ara", "Arabic"),
    ("fa", "pes", "Persian"),
    ("hi", "hin", "Hindi"),
    ("zh", "cmn", "Chinese (Simplified)"),
    ("ja", "jpn", "Japanese"),
    ("ko", "kor", "Korean"),
    ("vi", "vie", "Vietnamese"),
    ("th", "tha", "Thai"),
    ("id", "ind", "Indonesian"),
];

pub fn all_codes() -> Vec<String> {
    LANGUAGES.iter().map(|(c, _, _)| c.to_string()).collect()
}

pub fn is_supported(code: &str) -> bool {
    LANGUAGES.iter().any(|(c, _, _)| *c == normalize(code))
}

/// English name used inside LLM prompts.
pub fn english_name(code: &str) -> String {
    let code = normalize(code);
    LANGUAGES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, _, n)| n.to_string())
        .unwrap_or(code)
}

pub fn from_iso639_3(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(_, c3, _)| *c3 == code).map(|(c, _, _)| *c)
}

pub fn to_iso639_3(code: &str) -> Option<&'static str> {
    let code = normalize(code);
    LANGUAGES.iter().find(|(c, _, _)| *c == code).map(|(_, c3, _)| *c3)
}

/// "en-US" / "EN" / "pt_BR" → "en" / "pt"; also maps a few aliases.
pub fn normalize(code: &str) -> String {
    let base = code
        .split(['-', '_'])
        .next()
        .unwrap_or(code)
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "no" | "nn" => "nb".into(),
        "iw" => "he".into(),
        _ => base,
    }
}

/// Code for DeepL `target_lang` (some languages need a regional variant).
pub fn deepl_target(code: &str) -> String {
    match normalize(code).as_str() {
        "en" => "EN-US".into(),
        "pt" => "PT-BR".into(),
        "zh" => "ZH-HANS".into(),
        other => other.to_ascii_uppercase(),
    }
}

pub fn deepl_source(code: &str) -> String {
    normalize(code).to_ascii_uppercase()
}

/// Microsoft / Google use "zh-Hans" / "zh-CN" for simplified Chinese.
pub fn microsoft_code(code: &str) -> String {
    match normalize(code).as_str() {
        "zh" => "zh-Hans".into(),
        other => other.into(),
    }
}

pub fn google_code(code: &str) -> String {
    match normalize(code).as_str() {
        "zh" => "zh-CN".into(),
        "he" => "iw".into(),
        "nb" => "no".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize("en-US"), "en");
        assert_eq!(normalize("PT_br"), "pt");
        assert_eq!(normalize("no"), "nb");
        assert_eq!(deepl_target("en"), "EN-US");
    }
}
