//! Native speaker recordings of single words. English Wiktionary links audio
//! files from Wikimedia Commons (including Lingua Libre) for most languages,
//! often with the accent noted: `{{audio|en|En-uk-water.ogg|a=RP}}`.

use std::collections::HashMap;

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{Audio, AudioFormat, Error, Result};

const API: &str = "https://en.wiktionary.org/w/api.php";
const USER_AGENT: &str = concat!("1lang/", env!("CARGO_PKG_VERSION"), " (https://github.com/q-sn/1lang)");

#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    /// Commons file name, e.g. "En-us-water.ogg".
    pub file: String,
    pub accent: Option<String>,
}

impl Recording {
    pub fn page_url(&self) -> String {
        let mut url = reqwest::Url::parse("https://commons.wikimedia.org/wiki/").unwrap();
        url.path_segments_mut().unwrap().pop_if_empty().push(&format!("File:{}", self.file));
        url.to_string()
    }

    fn download_url(&self) -> reqwest::Url {
        let mut url = reqwest::Url::parse("https://commons.wikimedia.org/wiki/Special:FilePath/").unwrap();
        url.path_segments_mut().unwrap().pop_if_empty().push(&self.file);
        url
    }
}

pub struct Recordings {
    http: reqwest::Client,
    /// "lang|word" → recordings found on the page (empty if none).
    found: Mutex<HashMap<String, Vec<Recording>>>,
}

#[derive(Deserialize)]
struct ParseResponse {
    parse: Option<ParsePage>,
}

#[derive(Deserialize)]
struct ParsePage {
    wikitext: String,
}

/// Language section heading on English Wiktionary.
fn wiktionary_language(lang: &str) -> Option<&'static str> {
    Some(match lang {
        "en" => "English",
        "ru" => "Russian",
        "es" => "Spanish",
        "de" => "German",
        "fr" => "French",
        "it" => "Italian",
        "pt" => "Portuguese",
        "uk" => "Ukrainian",
        "be" => "Belarusian",
        "pl" => "Polish",
        "cs" => "Czech",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "da" => "Danish",
        "nb" => "Norwegian Bokmål",
        "fi" => "Finnish",
        "el" => "Greek",
        "hu" => "Hungarian",
        "ro" => "Romanian",
        "bg" => "Bulgarian",
        "sr" | "hr" => "Serbo-Croatian",
        "tr" => "Turkish",
        "kk" => "Kazakh",
        "uz" => "Uzbek",
        "ka" => "Georgian",
        "hy" => "Armenian",
        "he" => "Hebrew",
        "ar" => "Arabic",
        "fa" => "Persian",
        "hi" => "Hindi",
        "zh" => "Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        "vi" => "Vietnamese",
        "th" => "Thai",
        "id" => "Indonesian",
        _ => return None,
    })
}

/// Words that mark an accent label as belonging to a region.
fn region_keywords(region: &str) -> &'static [&'static str] {
    match region {
        "US" => &["us", "usa", "american", "genam", "general american"],
        "GB" => &["uk", "gb", "rp", "british", "england", "english", "london", "southern england"],
        "AU" => &["australia", "australian", "au"],
        "CA" => &["canada", "canadian", "ca", "quebec"],
        "NZ" => &["new zealand", "nz"],
        "IE" => &["ireland", "irish"],
        "IN" => &["india", "indian"],
        "ES" => &["spain", "castilian", "madrid", "es"],
        "MX" => &["mexico", "mexican", "mx"],
        "AR" => &["argentina", "rioplatense"],
        "CO" => &["colombia"],
        "BR" => &["brazil", "brazilian", "br", "são paulo", "rio de janeiro"],
        "PT" => &["portugal", "european", "lisbon", "pt"],
        "FR" => &["france", "paris", "fr"],
        "BE" => &["belgium"],
        "CH" => &["switzerland", "swiss"],
        "DE" => &["germany", "berlin", "de"],
        "AT" => &["austria", "vienna"],
        "CN" => &["mainland", "beijing", "putonghua"],
        "TW" => &["taiwan"],
        _ => &[],
    }
}

fn audio_file(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| AudioFormat::from_extension(ext).is_some())
}

/// The level-2 section of a language (`==Spanish==`) of a page's wikitext.
fn language_section<'a>(wikitext: &'a str, language: &str) -> Option<&'a str> {
    let heading = format!("=={language}==");
    let mut start = None;
    let mut offset = 0;
    for line in wikitext.split_inclusive('\n') {
        let t = line.trim();
        let level2 = t.starts_with("==") && !t.starts_with("===") && t.ends_with("==") && !t.ends_with("===");
        if let Some(s) = start {
            if level2 {
                return Some(&wikitext[s..offset]);
            }
        } else if level2 && t.replace(' ', "") == heading.replace(' ', "") {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start.map(|s| &wikitext[s..])
}

/// Audio files of a section: `{{audio|xx|File.ogg|a=US}}` templates and
/// `<audio:File.ogg<a:Spain>>` modifiers of pronunciation templates.
fn extract(section: &str) -> Vec<Recording> {
    let mut out: Vec<Recording> = vec![];
    let mut push = |file: &str, accent: Option<String>| {
        let file = file.trim().replace('_', " ");
        if audio_file(&file) && !out.iter().any(|r| r.file == file) {
            out.push(Recording { file, accent: accent.filter(|a| !a.is_empty()) });
        }
    };

    let mut rest = section;
    while let Some(i) = rest.find("{{audio|") {
        let body = &rest[i + 8..];
        let end = body.find("}}").unwrap_or(body.len());
        let params: Vec<&str> = body[..end].split('|').collect();
        if let Some(file) = params.get(1) {
            let accent = params.iter().skip(2).find_map(|p| p.trim().strip_prefix("a=")).map(clean_label);
            push(file, accent);
        }
        rest = &body[end..];
    }

    let mut rest = section;
    while let Some(i) = rest.find("<audio:") {
        let body = &rest[i + 7..];
        let end = body.find(['<', '>', ';', '#']).unwrap_or(body.len());
        let accent = body[end..]
            .strip_prefix("<a:")
            .and_then(|a| a.split_once('>'))
            .map(|(a, _)| clean_label(a));
        push(&body[..end], accent);
        rest = &body[end..];
    }
    out
}

/// "<<France>> (<<Paris>>)" → "France (Paris)".
fn clean_label(s: &str) -> String {
    s.replace("<<", "").replace(">>", "").replace("[[", "").replace("]]", "").trim().to_string()
}

fn contains_word(haystack: &str, word: &str) -> bool {
    haystack.match_indices(word).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Best recording for a preferred region ("US", "GB", …).
fn pick<'a>(list: &'a [Recording], lang: &str, region: Option<&str>) -> Option<&'a Recording> {
    let score = |r: &Recording| -> i32 {
        let mut s = 0;
        let file = r.file.to_lowercase();
        if let Some(region) = region {
            let region_lc = region.to_lowercase();
            let alias = if region == "GB" { "uk" } else { region_lc.as_str() };
            let file_match = file.starts_with(&format!("{lang}-{region_lc}-")) || file.starts_with(&format!("{lang}-{alias}-"));
            let label = r.accent.as_deref().unwrap_or("").to_lowercase();
            let label_match = region_keywords(region).iter().any(|k| contains_word(&label, k));
            // "En-au-…": named after some region.
            let file_region = file
                .strip_prefix(&format!("{lang}-"))
                .is_some_and(|rest| rest.len() > 3 && rest.as_bytes()[2] == b'-' && rest[..2].chars().all(|c| c.is_ascii_alphabetic()));
            if file_match || label_match {
                s += 4;
            } else if !label.is_empty() || file_region {
                // Some other accent.
                s -= 2;
            }
        }
        // Curated recordings tend to be cleaner than crowd-sourced ones.
        if !file.starts_with("ll-") {
            s += 1;
        }
        s
    };
    list.iter().enumerate().max_by_key(|(i, r)| (score(r), -(*i as i32))).map(|(_, r)| r)
}

impl Recordings {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, found: Mutex::new(HashMap::new()) }
    }

    async fn page_recordings(&self, title: &str, lang: &str) -> Result<Vec<Recording>> {
        let Some(language) = wiktionary_language(lang) else {
            return Ok(vec![]);
        };
        let key = format!("{lang}|{title}");
        if let Some(found) = self.found.lock().await.get(&key) {
            return Ok(found.clone());
        }
        let resp: ParseResponse = self
            .http
            .get(API)
            .query(&[
                ("action", "parse"),
                ("page", title),
                ("prop", "wikitext"),
                ("redirects", "1"),
                ("format", "json"),
                ("formatversion", "2"),
            ])
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .json()
            .await?;
        // A missing page comes back as an error object without `parse`.
        let found = resp
            .parse
            .and_then(|p| language_section(&p.wikitext, language).map(extract))
            .unwrap_or_default();
        self.found.lock().await.insert(key, found.clone());
        Ok(found)
    }

    /// Find a recording of `word`, preferring the accent of `region`.
    pub async fn find(&self, word: &str, lang: &str, region: Option<&str>) -> Result<Option<Recording>> {
        let word = word.trim();
        let mut titles = vec![word.to_string()];
        let lower = word.to_lowercase();
        if lower != word {
            titles.push(lower);
        }
        for title in titles {
            let list = self.page_recordings(&title, lang).await?;
            if let Some(r) = pick(&list, lang, region) {
                return Ok(Some(r.clone()));
            }
        }
        Ok(None)
    }

    pub async fn download(&self, r: &Recording) -> Result<Audio> {
        let format = r
            .file
            .rsplit_once('.')
            .and_then(|(_, ext)| AudioFormat::from_extension(ext))
            .ok_or_else(|| Error::Service(format!("unsupported audio file {}", r.file)))?;
        let resp = self.http.get(r.download_url()).header("User-Agent", USER_AGENT).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Service(format!("recording: HTTP {}", resp.status())));
        }
        Ok(Audio { bytes: resp.bytes().await?.to_vec(), format })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATER: &str = "==English==\n===Pronunciation===\n\
        * {{audio|en|En-uk-water.ogg|a=RP}}\n\
        * {{audio|en|LL-Q1860 (eng)-Nail123Real-water.wav|a=US,non-cot-caught}}\n\
        * {{audio|en|En-us-water.ogg|a=US,cot-caught}}\n\
        ==Dutch==\n* {{audio|nl|Nl-water (Netherlands).ogg|a=Netherlands}}\n\
        ==Spanish==\n{{es-pr|+<audio:Hola.ogg><audio:LL-Q1321 (spa)-X-hola.wav<a:Mexico>>}}\n";

    #[test]
    fn sections_and_files() {
        let en = extract(language_section(WATER, "English").unwrap());
        assert_eq!(en.len(), 3);
        assert_eq!(en[0], Recording { file: "En-uk-water.ogg".into(), accent: Some("RP".into()) });
        let nl = extract(language_section(WATER, "Dutch").unwrap());
        assert_eq!(nl[0].file, "Nl-water (Netherlands).ogg");
        let es = extract(language_section(WATER, "Spanish").unwrap());
        assert_eq!(es.len(), 2);
        assert_eq!(es[1].accent.as_deref(), Some("Mexico"));
        assert!(language_section(WATER, "French").is_none());
    }

    #[test]
    fn accent_preference() {
        let en = extract(language_section(WATER, "English").unwrap());
        assert_eq!(pick(&en, "en", Some("US")).unwrap().file, "En-us-water.ogg");
        assert_eq!(pick(&en, "en", Some("GB")).unwrap().file, "En-uk-water.ogg");
        let es = extract(language_section(WATER, "Spanish").unwrap());
        assert_eq!(pick(&es, "es", Some("ES")).unwrap().file, "Hola.ogg");
        assert_eq!(pick(&es, "es", Some("MX")).unwrap().file, "LL-Q1321 (spa)-X-hola.wav");
        assert!(pick(&[], "en", None).is_none());
    }

    #[test]
    fn urls() {
        let r = Recording { file: "LL-Q1860 (eng)-A-water.wav".into(), accent: None };
        assert_eq!(r.page_url(), "https://commons.wikimedia.org/wiki/File:LL-Q1860%20(eng)-A-water.wav");
    }
}
