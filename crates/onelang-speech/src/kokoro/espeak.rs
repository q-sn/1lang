//! Text → Kokoro phonemes with the espeak-ng command line tool, converted to
//! Kokoro's phoneme set the way its own G2P library (misaki) does.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::{Error, Result};

pub struct Espeak {
    /// Folder with espeak-ng.exe, its DLL and espeak-ng-data.
    dir: PathBuf,
}

enum Segment {
    Text(String),
    Punct(char),
}

/// Punctuation Kokoro understands (others only split the text).
fn punct(c: char) -> Option<Option<char>> {
    Some(match c {
        ';' | ':' | ',' | '.' | '!' | '?' | '—' | '…' | '"' | '(' | ')' | '“' | '”' => Some(c),
        '«' => Some('“'),
        '»' => Some('”'),
        '–' => Some('—'),
        '¡' | '¿' | '[' | ']' | '{' | '}' => None,
        _ => return None,
    })
}

fn segments(text: &str) -> Vec<Segment> {
    let mut out = vec![];
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<Segment>| {
        if cur.chars().any(char::is_alphanumeric) {
            out.push(Segment::Text(cur.split_whitespace().collect::<Vec<_>>().join(" ")));
        }
        cur.clear();
    };
    for c in text.chars() {
        match punct(c) {
            Some(mark) => {
                flush(&mut cur, &mut out);
                if let Some(m) = mark {
                    out.push(Segment::Punct(m));
                }
            }
            None => cur.push(c),
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// espeak IPA (with "^" ties) → Kokoro phonemes, as misaki's EspeakFallback.
fn english(ps: &str, british: bool) -> String {
    const E2M: &[(&str, &str)] = &[
        ("ʔˌn\u{329}", "ʔn"),
        ("ʔn\u{329}", "ʔn"),
        ("a^ɪ", "I"),
        ("a^ʊ", "W"),
        ("d^ʒ", "ʤ"),
        ("e^ɪ", "A"),
        ("t^ʃ", "ʧ"),
        ("ɔ^ɪ", "Y"),
        ("ə^l", "ᵊl"),
        ("ʲo", "jo"),
        ("ʲə", "jə"),
        ("\u{303}", ""),
        ("e", "A"),
        ("ʲ", ""),
        ("ɚ", "əɹ"),
        ("r", "ɹ"),
        ("x", "k"),
        ("ç", "k"),
        ("ɐ", "ə"),
        ("ɬ", "l"),
    ];
    let mut ps = ps.to_string();
    for (old, new) in E2M {
        ps = ps.replace(old, new);
    }
    ps = syllabic(&ps);
    if british {
        ps = ps.replace("e^ə", "ɛː").replace("iə", "ɪə").replace("ə^ʊ", "Q");
    } else {
        ps = ps
            .replace("o^ʊ", "O")
            .replace("ɜːɹ", "ɜɹ")
            .replace("ɜː", "ɜɹ")
            .replace("ɪə", "iə")
            .replace('ː', "");
    }
    ps.replace('o', "ɔ").replace('ɾ', "T").replace('ʔ', "t").replace('^', "")
}

/// espeak IPA → Kokoro phonemes for other languages, as misaki's EspeakG2P.
fn other(ps: &str) -> String {
    const E2M: &[(&str, &str)] = &[
        ("a^ɪ", "I"),
        ("a^ʊ", "W"),
        ("d^z", "ʣ"),
        ("d^ʒ", "ʤ"),
        ("e^ɪ", "A"),
        ("o^ʊ", "O"),
        ("ə^ʊ", "Q"),
        ("s^s", "S"),
        ("t^s", "ʦ"),
        ("t^ʃ", "ʧ"),
        ("ɔ^ɪ", "Y"),
    ];
    let mut ps = ps.to_string();
    for (old, new) in E2M {
        ps = ps.replace(old, new);
    }
    ps.replace(['^', '-'], "")
}

/// "n̩" (syllabic) → "ᵊn".
fn syllabic(ps: &str) -> String {
    let chars: Vec<char> = ps.chars().collect();
    let mut out = String::with_capacity(ps.len());
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i + 1] == '\u{329}' && !chars[i].is_whitespace() {
            out.push('ᵊ');
            out.push(chars[i]);
            i += 2;
        } else {
            if chars[i] != '\u{329}' {
                out.push(chars[i]);
            }
            i += 1;
        }
    }
    out
}

/// Join phonemized text and punctuation: "həlˈO, wˈɜɹld."
fn assemble(parts: &[(bool, String)]) -> String {
    let mut out = String::new();
    for (is_text, s) in parts {
        let opening = matches!(s.as_str(), "(" | "“");
        if (*is_text || opening) && !out.is_empty() && !out.ends_with([' ', '(', '“']) {
            out.push(' ');
        }
        out.push_str(s);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Espeak {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn exe(&self) -> PathBuf {
        self.dir.join("espeak-ng.exe")
    }

    pub fn is_installed(&self) -> bool {
        self.exe().is_file() && self.dir.join("espeak-ng-data").is_dir()
    }

    /// Phonemes of `input` lines (one output line per paragraph).
    async fn run(&self, voice: &str, input: &str) -> Result<Vec<String>> {
        let mut cmd = Command::new(self.exe());
        cmd.args(["-q", "--ipa", "--tie=^", "-v", voice, "--path"])
            .arg(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn().map_err(|e| Error::Model(format!("espeak-ng: {e}")))?;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input.as_bytes()).await?;
        drop(stdin);
        let out = child.wait_with_output().await?;
        if !out.status.success() {
            return Err(Error::Model(format!("espeak-ng: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Kokoro phonemes of `text` for a voice locale ("en-GB", "pt-BR", …).
    pub async fn phonemize(&self, text: &str, locale: &str) -> Result<String> {
        let voice = match locale {
            "en-US" => "en-us",
            "en-GB" => "en-gb",
            "fr-FR" => "fr-fr",
            "pt-BR" => "pt-br",
            other => crate::lang_of(other),
        };
        let convert = |ps: &str| match locale {
            "en-US" => english(ps, false),
            "en-GB" => english(ps, true),
            _ => other(ps),
        };

        let segs = segments(text);
        let texts: Vec<&str> = segs
            .iter()
            .filter_map(|s| match s {
                Segment::Text(t) => Some(t.as_str()),
                Segment::Punct(_) => None,
            })
            .collect();
        if texts.is_empty() {
            return Err(Error::Empty);
        }
        // All pieces in one run, separated as paragraphs; if espeak split a
        // piece further, phonemize the pieces one by one.
        let mut phonemes = self.run(voice, &texts.join("\n\n")).await?;
        if phonemes.len() != texts.len() {
            phonemes.clear();
            for t in &texts {
                phonemes.push(self.run(voice, t).await?.join(" "));
            }
        }

        let mut next = phonemes.iter();
        let parts: Vec<(bool, String)> = segs
            .iter()
            .map(|s| match s {
                Segment::Text(_) => (true, convert(next.next().map(String::as_str).unwrap_or(""))),
                Segment::Punct(c) => (false, c.to_string()),
            })
            .collect();
        Ok(assemble(&parts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_punctuation() {
        let s = segments("Hello, «world»! (ok) -- ¿qué?");
        let shown: Vec<String> = s
            .iter()
            .map(|s| match s {
                Segment::Text(t) => format!("[{t}]"),
                Segment::Punct(c) => c.to_string(),
            })
            .collect();
        assert_eq!(shown.join(""), "[Hello],“[world]”!([ok])[qué]?");
    }

    #[test]
    fn english_mapping() {
        assert_eq!(english("həlˈo^ʊ", false), "həlˈO");
        assert_eq!(english("wˈɔːɾɚ", false), "wˈɔTəɹ");
        assert_eq!(english("bˈɑːɾə^l", false), "bˈɑTᵊl");
        assert_eq!(english("ðˈe^ɪ", true), "ðˈA");
        assert_eq!(english("bˈʌʔn\u{329}", false), "bˈʌtn");
    }

    #[test]
    fn joins() {
        let parts = vec![
            (true, "həlˈO".to_string()),
            (false, ",".to_string()),
            (true, "wˈɜɹld".to_string()),
            (false, "(".to_string()),
            (true, "ɔk".to_string()),
            (false, ")".to_string()),
            (false, ".".to_string()),
        ];
        assert_eq!(assemble(&parts), "həlˈO, wˈɜɹld (ɔk).");
    }
}
