//! Microsoft Edge "Read aloud" voices: the same neural voices as Azure Speech,
//! through the endpoint the browser uses. Free and keyless, but unofficial:
//! the browser emulation (version, `Sec-MS-GEC` token) may need updates when
//! Microsoft changes it. Protocol as implemented by github.com/rany2/edge-tts.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::{lang_of, Audio, AudioFormat, Engine, Error, Result, Synthesizer, Voice};

const BASE_URL: &str = "speech.platform.bing.com/consumer/speech/synthesize/readaloud";
const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
const CHROMIUM_FULL_VERSION: &str = "143.0.3650.75";
const CHROMIUM_MAJOR_VERSION: &str = "143";
const ORIGIN: &str = "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold";
/// Max SSML text per request, bytes.
const MAX_CHUNK: usize = 4096;
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ATTEMPTS: u32 = 6;

pub struct EdgeTts {
    http: reqwest::Client,
    voices: Mutex<Option<Vec<Voice>>>,
    /// Server time minus local time, seconds (the token is time-based).
    clock_skew: AtomicI64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VoiceJson {
    short_name: String,
    locale: String,
    gender: Option<String>,
}

fn user_agent() -> String {
    format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
         Chrome/{CHROMIUM_MAJOR_VERSION}.0.0.0 Safari/537.36 Edg/{CHROMIUM_MAJOR_VERSION}.0.0.0"
    )
}

fn random_hex() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// "en-US-AvaMultilingualNeural" → "Ava (Multilingual)".
fn display_name(short_name: &str) -> String {
    let last = short_name.rsplit('-').next().unwrap_or(short_name);
    let base = last.strip_suffix("Neural").unwrap_or(last);
    match base.strip_suffix("Multilingual") {
        Some(n) => format!("{n} (Multilingual)"),
        None => base.to_string(),
    }
}

/// Keep the text the service accepts and XML-escape it.
fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Split escaped text into chunks of at most `max` bytes, at newlines or
/// spaces when possible, never inside a character or an XML entity.
fn split_chunks(text: &str, max: usize) -> Vec<String> {
    let mut chunks = vec![];
    let mut rest = text.trim();
    while rest.len() > max {
        let mut end = max;
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let head = &rest[..end];
        let mut at = head.rfind('\n').or_else(|| head.rfind(' ')).filter(|&i| i > 0).unwrap_or(end);
        if let Some(amp) = rest[..at].rfind('&') {
            if !rest[amp..at].contains(';') {
                at = amp.max(1);
            }
        }
        let chunk = rest[..at].trim();
        if !chunk.is_empty() {
            chunks.push(chunk.to_string());
        }
        rest = rest[at..].trim_start();
    }
    if !rest.is_empty() {
        chunks.push(rest.to_string());
    }
    chunks
}

/// JavaScript-style UTC date, as Edge sends it.
fn js_date(unix: i64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{} {} {:02} {} {:02}:{:02}:{:02} GMT+0000 (Coordinated Universal Time)",
        DAYS[days.rem_euclid(7) as usize],
        MONTHS[(month - 1) as usize],
        day,
        year,
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn split_headers(text: &str) -> (Option<&str>, &str) {
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text, ""));
    let path = head.lines().find_map(|l| l.strip_prefix("Path:")).map(str::trim);
    (path, body)
}

impl EdgeTts {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, voices: Mutex::new(None), clock_skew: AtomicI64::new(0) }
    }

    fn now(&self) -> i64 {
        let local = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        local + self.clock_skew.load(Ordering::Relaxed)
    }

    /// Time-based token required by the service: SHA-256 of the Windows file
    /// time rounded down to 5 minutes, followed by the client token.
    fn sec_ms_gec(&self) -> String {
        const WIN_EPOCH: i64 = 11_644_473_600;
        let mut secs = self.now() + WIN_EPOCH;
        secs -= secs % 300;
        let ticks = secs as u128 * 10_000_000;
        let digest = Sha256::digest(format!("{ticks}{TRUSTED_CLIENT_TOKEN}").as_bytes());
        digest.iter().map(|b| format!("{b:02X}")).collect()
    }

    fn token_query(&self) -> String {
        format!("Sec-MS-GEC={}&Sec-MS-GEC-Version=1-{CHROMIUM_FULL_VERSION}", self.sec_ms_gec())
    }

    /// A 403 usually means the local clock is off: adopt the server's time.
    fn adjust_clock(&self, date: Option<&str>) -> bool {
        let Some(server) = date.and_then(|d| httpdate::parse_http_date(d).ok()) else {
            return false;
        };
        let server = server.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let skew = server - (self.now() - self.clock_skew.load(Ordering::Relaxed));
        self.clock_skew.store(skew, Ordering::Relaxed);
        log::info!("edge tts: clock skew adjusted to {skew}s");
        true
    }

    async fn fetch_voices(&self) -> Result<Vec<Voice>> {
        let mut retried = false;
        loop {
            let url = format!(
                "https://{BASE_URL}/voices/list?trustedclienttoken={TRUSTED_CLIENT_TOKEN}&{}",
                self.token_query()
            );
            let resp = self
                .http
                .get(url)
                .header("User-Agent", user_agent())
                .header("Accept", "*/*")
                .header("Accept-Language", "en-US,en;q=0.9")
                .header(
                    "Sec-CH-UA",
                    format!(
                        "\" Not;A Brand\";v=\"99\", \"Microsoft Edge\";v=\"{CHROMIUM_MAJOR_VERSION}\", \"Chromium\";v=\"{CHROMIUM_MAJOR_VERSION}\""
                    ),
                )
                .header("Sec-CH-UA-Mobile", "?0")
                .header("Sec-Fetch-Site", "none")
                .header("Sec-Fetch-Mode", "cors")
                .header("Sec-Fetch-Dest", "empty")
                .send()
                .await?;
            if resp.status() == reqwest::StatusCode::FORBIDDEN && !retried {
                retried = true;
                let date = resp.headers().get("date").and_then(|v| v.to_str().ok()).map(str::to_string);
                if self.adjust_clock(date.as_deref()) {
                    continue;
                }
            }
            if !resp.status().is_success() {
                return Err(Error::Service(format!("voice list: HTTP {}", resp.status())));
            }
            let list: Vec<VoiceJson> = resp.json().await?;
            return Ok(list
                .into_iter()
                .map(|v| Voice {
                    name: display_name(&v.short_name),
                    id: v.short_name,
                    locale: v.locale,
                    gender: v.gender,
                    engine: Engine::Edge,
                })
                .collect());
        }
    }

    async fn synthesize_chunk(&self, ssml_text: &str, voice: &str, rate: f32) -> Result<Vec<u8>> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match tokio::time::timeout(TIMEOUT, self.request(ssml_text, voice, rate)).await {
                Err(_) => return Err(Error::Network("timed out".into())),
                // Besides clock skew, the service rejects a share of requests
                // at random (since its endpoint migration, Oct 2026): retry.
                Ok(Err(tungstenite::Error::Http(resp))) if resp.status() == 403 && attempt < MAX_ATTEMPTS => {
                    if attempt == 1 {
                        self.adjust_clock(resp.headers().get("date").and_then(|v| v.to_str().ok()));
                    }
                    tokio::time::sleep(Duration::from_millis(150 * attempt as u64)).await;
                }
                Ok(Err(tungstenite::Error::Http(resp))) => {
                    return Err(Error::Service(format!("HTTP {}", resp.status())));
                }
                Ok(Err(e)) => return Err(Error::Network(e.to_string())),
                Ok(Ok(audio)) if audio.is_empty() => return Err(Error::Service("no audio received".into())),
                Ok(Ok(audio)) => return Ok(audio),
            }
        }
    }

    async fn request(&self, ssml_text: &str, voice: &str, rate: f32) -> std::result::Result<Vec<u8>, tungstenite::Error> {
        let url = format!(
            "wss://{BASE_URL}/edge/v1?TrustedClientToken={TRUSTED_CLIENT_TOKEN}&ConnectionId={}&{}",
            random_hex(),
            self.token_query()
        );
        let mut req = url.into_client_request()?;
        let headers = req.headers_mut();
        let mut set = |name: &'static str, value: String| {
            if let Ok(v) = HeaderValue::from_str(&value) {
                headers.insert(name, v);
            }
        };
        set("Pragma", "no-cache".into());
        set("Cache-Control", "no-cache".into());
        set("Origin", ORIGIN.into());
        set("User-Agent", user_agent());
        set("Accept-Language", "en-US,en;q=0.9".into());
        set("Cookie", format!("muid={};", random_hex().to_uppercase()));

        let (mut ws, _) = tokio_tungstenite::connect_async(req).await?;
        let date = js_date(self.now());
        ws.send(Message::text(format!(
            "X-Timestamp:{date}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n\
             {{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\
             \"wordBoundaryEnabled\":\"false\"}},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}}}}}\r\n"
        )))
        .await?;

        let percent = ((rate - 1.0) * 100.0).round() as i32;
        let locale = voice.splitn(3, '-').take(2).collect::<Vec<_>>().join("-");
        let ssml = format!(
            "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='{locale}'>\
             <voice name='{voice}'><prosody pitch='+0Hz' rate='{percent:+}%' volume='+0%'>{ssml_text}</prosody></voice></speak>"
        );
        // The trailing "Z" mirrors a quirk of Edge itself.
        ws.send(Message::text(format!(
            "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{date}Z\r\nPath:ssml\r\n\r\n{ssml}",
            random_hex()
        )))
        .await?;

        let mut audio = vec![];
        while let Some(msg) = ws.next().await {
            match msg? {
                Message::Text(text) => {
                    if split_headers(&text).0 == Some("turn.end") {
                        break;
                    }
                }
                Message::Binary(data) => {
                    if data.len() < 2 {
                        continue;
                    }
                    let header_len = u16::from_be_bytes([data[0], data[1]]) as usize;
                    if data.len() < header_len + 2 {
                        continue;
                    }
                    let head = String::from_utf8_lossy(&data[2..2 + header_len]);
                    if head.contains("Path:audio") {
                        audio.extend_from_slice(&data[2 + header_len..]);
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        let _ = ws.close(None).await;
        Ok(audio)
    }
}

#[async_trait]
impl Synthesizer for EdgeTts {
    fn engine(&self) -> Engine {
        Engine::Edge
    }

    async fn voices(&self) -> Result<Vec<Voice>> {
        let mut cached = self.voices.lock().await;
        if let Some(v) = cached.as_ref() {
            return Ok(v.clone());
        }
        let list = self.fetch_voices().await?;
        *cached = Some(list.clone());
        Ok(list)
    }

    async fn synthesize(&self, text: &str, voice: &str, rate: f32) -> Result<Audio> {
        let chunks = split_chunks(&clean_text(text), MAX_CHUNK);
        if chunks.is_empty() {
            return Err(Error::Empty);
        }
        // MP3 frames can simply be concatenated.
        let mut bytes = vec![];
        for chunk in &chunks {
            bytes.extend(self.synthesize_chunk(chunk, voice, rate.clamp(0.5, 2.0)).await?);
        }
        Ok(Audio { bytes, format: AudioFormat::Mp3 })
    }
}

/// Preferred voice per language: (language, locale, voice).
const DEFAULT_VOICES: &[(&str, &str, &str)] = &[
    ("en", "en-US", "en-US-AriaNeural"),
    ("ru", "ru-RU", "ru-RU-SvetlanaNeural"),
    ("es", "es-ES", "es-ES-ElviraNeural"),
    ("de", "de-DE", "de-DE-KatjaNeural"),
    ("fr", "fr-FR", "fr-FR-DeniseNeural"),
    ("it", "it-IT", "it-IT-ElsaNeural"),
    ("pt", "pt-BR", "pt-BR-FranciscaNeural"),
    ("uk", "uk-UA", "uk-UA-PolinaNeural"),
    ("pl", "pl-PL", "pl-PL-ZofiaNeural"),
    ("cs", "cs-CZ", "cs-CZ-VlastaNeural"),
    ("nl", "nl-NL", "nl-NL-ColetteNeural"),
    ("sv", "sv-SE", "sv-SE-SofieNeural"),
    ("da", "da-DK", "da-DK-ChristelNeural"),
    ("nb", "nb-NO", "nb-NO-PernilleNeural"),
    ("fi", "fi-FI", "fi-FI-NooraNeural"),
    ("el", "el-GR", "el-GR-AthinaNeural"),
    ("hu", "hu-HU", "hu-HU-NoemiNeural"),
    ("ro", "ro-RO", "ro-RO-AlinaNeural"),
    ("bg", "bg-BG", "bg-BG-KalinaNeural"),
    ("sr", "sr-RS", "sr-RS-SophieNeural"),
    ("hr", "hr-HR", "hr-HR-GabrijelaNeural"),
    ("tr", "tr-TR", "tr-TR-EmelNeural"),
    ("kk", "kk-KZ", "kk-KZ-AigulNeural"),
    ("uz", "uz-UZ", "uz-UZ-MadinaNeural"),
    ("ka", "ka-GE", "ka-GE-EkaNeural"),
    ("he", "he-IL", "he-IL-HilaNeural"),
    ("ar", "ar-EG", "ar-EG-SalmaNeural"),
    ("fa", "fa-IR", "fa-IR-DilaraNeural"),
    ("hi", "hi-IN", "hi-IN-SwaraNeural"),
    ("zh", "zh-CN", "zh-CN-XiaoxiaoNeural"),
    ("ja", "ja-JP", "ja-JP-NanamiNeural"),
    ("ko", "ko-KR", "ko-KR-SunHiNeural"),
    ("vi", "vi-VN", "vi-VN-HoaiMyNeural"),
    ("th", "th-TH", "th-TH-PremwadeeNeural"),
    ("id", "id-ID", "id-ID-GadisNeural"),
];

/// Child voices sound odd for learning.
const SKIP_BY_DEFAULT: &[&str] = &["en-US-AnaNeural", "zh-CN-XiaoyouNeural"];

/// Default voice for a language. `voices` is the list from the service, if
/// it could be loaded; without it the built-in choice is used blindly.
pub fn default_voice(lang: &str, voices: Option<&[Voice]>) -> Option<String> {
    let preset = DEFAULT_VOICES.iter().find(|(l, _, _)| *l == lang);
    let Some(voices) = voices else {
        return preset.map(|(_, _, v)| v.to_string());
    };
    if let Some((_, _, v)) = preset {
        if voices.iter().any(|x| x.id == *v) {
            return Some(v.to_string());
        }
    }
    let usable = |v: &&Voice| v.lang() == lang && !SKIP_BY_DEFAULT.contains(&v.id.as_str());
    let monolingual = |v: &&Voice| !v.id.contains("Multilingual");
    preset
        .and_then(|(_, locale, _)| voices.iter().filter(usable).filter(monolingual).find(|v| v.locale == *locale))
        .or_else(|| voices.iter().filter(usable).find(monolingual))
        .or_else(|| voices.iter().find(usable))
        .map(|v| v.id.clone())
}

/// Locale of an Edge voice id: "en-GB-SoniaNeural" → "en-GB".
pub fn voice_locale(id: &str) -> String {
    id.splitn(3, '-').take(2).collect::<Vec<_>>().join("-")
}

/// Language of an Edge voice id.
pub fn voice_lang(id: &str) -> &str {
    lang_of(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_date_format() {
        assert_eq!(js_date(0), "Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)");
        assert_eq!(js_date(1_791_000_000), "Sat Oct 03 2026 04:00:00 GMT+0000 (Coordinated Universal Time)");
    }

    #[test]
    fn chunks_respect_limits() {
        let text = "a &amp; b ".repeat(10);
        for c in split_chunks(&text, 12) {
            assert!(c.len() <= 12, "{c}");
            assert_eq!(c.matches('&').count(), c.matches(';').count(), "{c}");
        }
        assert_eq!(split_chunks("  hello  ", 100), vec!["hello"]);
        assert!(split_chunks("   ", 100).is_empty());
        let cyr = "привет ".repeat(20);
        assert!(split_chunks(&cyr, 15).iter().all(|c| c.len() <= 15));
    }

    #[test]
    fn names() {
        assert_eq!(display_name("en-US-AvaMultilingualNeural"), "Ava (Multilingual)");
        assert_eq!(display_name("zh-CN-liaoning-XiaobeiNeural"), "Xiaobei");
        assert_eq!(voice_locale("en-GB-SoniaNeural"), "en-GB");
    }

    #[test]
    fn default_voice_fallbacks() {
        let v = |id: &str, locale: &str| Voice {
            id: id.into(),
            name: id.into(),
            locale: locale.into(),
            gender: None,
            engine: Engine::Edge,
        };
        let list = vec![v("en-US-AnaNeural", "en-US"), v("en-US-GuyNeural", "en-US"), v("hy-AM-AnahitNeural", "hy-AM")];
        assert_eq!(default_voice("en", Some(&list)).as_deref(), Some("en-US-GuyNeural"));
        assert_eq!(default_voice("hy", Some(&list)).as_deref(), Some("hy-AM-AnahitNeural"));
        assert_eq!(default_voice("be", Some(&list)), None);
        assert_eq!(default_voice("en", None).as_deref(), Some("en-US-AriaNeural"));
    }
}
