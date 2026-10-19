//! Microsoft Edge "Read aloud" voices: the same neural voices as Azure Speech,
//! through the endpoint the browser uses. Free and keyless, but unofficial:
//! the browser emulation (version, `Sec-MS-GEC` token) may need updates when
//! Microsoft changes it. Protocol as implemented by github.com/rany2/edge-tts.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
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

// Sec-MS-GEC v2 token (since Oct 2026): "<CS64>-<SHA256>" of the Windows
// file time followed by the client token. The SHA-256 runs over a fixed
// 57-byte prefix that is only known as a midstate after 14 rounds of the
// first block. Constants as in edge-tts 7.3.1 (drm.py).

const SHA256_IV: [u32; 8] =
    [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19];
/// Working registers after round 13 of the first block.
const PREFIX_STATE: [u32; 8] =
    [0xFD9FBEA2, 0x91243CE7, 0xBE499094, 0xCD2E1E0A, 0x6C006F49, 0x04BD35AB, 0xFEB0A7E4, 0x03EFFBD2];
/// Prefix-only parts of schedule words 16..29.
const PREFIX_SCHEDULE: [u32; 14] = [
    0xEB24BF9E, 0x1558691F, 0xA0C09EE2, 0x0CFA32BA, 0xA622EE8C, 0xD1DD9491, 0xC0DD25A0, 0xF9E70AB6, 0xF0C31573,
    0x7058CE4E, 0x7188E606, 0x00E5AB33, 0xD258B060, 0x72766564,
];
/// K[14] plus the last prefix byte of word 14.
const PREFIX_K14: u32 = 0xC9DC06A7;
const PREFIX_SIGMA0: u32 = 0x059C0B80;
const PREFIX_SIGMA1: u32 = 0x000B92C0;
const K: [u32; 64] = [
    0x428A2F98, 0x71374491, 0xB5C0FBCF, 0xE9B5DBA5, 0x3956C25B, 0x59F111F1, 0x923F82A4, 0xAB1C5ED5, 0xD807AA98,
    0x12835B01, 0x243185BE, 0x550C7DC3, 0x72BE5D74, 0x80DEB1FE, 0x9BDC06A7, 0xC19BF174, 0xE49B69C1, 0xEFBE4786,
    0x0FC19DC6, 0x240CA1CC, 0x2DE92C6F, 0x4A7484AA, 0x5CB0A9DC, 0x76F988DA, 0x983E5152, 0xA831C66D, 0xB00327C8,
    0xBF597FC7, 0xC6E00BF3, 0xD5A79147, 0x06CA6351, 0x14292967, 0x27B70A85, 0x2E1B2138, 0x4D2C6DFC, 0x53380D13,
    0x650A7354, 0x766A0ABB, 0x81C2C92E, 0x92722C85, 0xA2BFE8A1, 0xA81A664B, 0xC24B8B70, 0xC76C51A3, 0xD192E819,
    0xD6990624, 0xF40E3585, 0x106AA070, 0x19A4C116, 0x1E376C08, 0x2748774C, 0x34B0BCB5, 0x391C0CB3, 0x4ED8AA4A,
    0x5B9CCA4F, 0x682E6FF3, 0x748F82EE, 0x78A5636F, 0x84C87814, 0x8CC70208, 0x90BEFFFA, 0xA4506CEB, 0xBEF9A3F7,
    0xC67178F2,
];
const CS64_COEFFICIENTS: [(u32, u32); 14] = [
    (0xE82E91B1, 0xB32700D0),
    (0x44DE24C6, 0x88DFF51D),
    (0x3BEB65BC, 0x29514038),
    (0xC89C5933, 0xB328CBDF),
    (0x8D1EC211, 0xD405B4D5),
    (0x314EF5D5, 0x26E55F3C),
    (0xE92491B1, 0x607801C6),
    (0xF0A5B9D5, 0x82B2C1FB),
    (0x06064CF3, 0xFA51A3DA),
    (0xDA79FA60, 0x207CC464),
    (0xBED20560, 0x3C3C74F3),
    (0x89F9C9F8, 0xD2D14B61),
    (0x35FA38C6, 0xBB04A2C1),
    (0xCB49E3C6, 0xC5626167),
];

fn sigma0(x: u32) -> u32 {
    x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
}

fn sigma1(x: u32) -> u32 {
    x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
}

/// Fill schedule words `from..64`.
fn expand(w: &mut [u32; 64], from: usize) {
    for i in from..64 {
        w[i] = sigma1(w[i - 2]).wrapping_add(w[i - 7]).wrapping_add(sigma0(w[i - 15])).wrapping_add(w[i - 16]);
    }
}

/// SHA-256 rounds `from..64` on working registers `s`.
fn rounds(mut s: [u32; 8], w: &[u32; 64], k: &[u32; 64], from: usize) -> [u32; 8] {
    for i in from..64 {
        let [a, b, c, d, e, f, g, h] = s;
        let t1 = h
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add(g ^ (e & (f ^ g)))
            .wrapping_add(k[i])
            .wrapping_add(w[i]);
        let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22)).wrapping_add((a & b) | (c & (a | b)));
        s = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
    }
    s
}

fn add(x: [u32; 8], y: [u32; 8]) -> [u32; 8] {
    std::array::from_fn(|i| x[i].wrapping_add(y[i]))
}

/// SHA-256 of the hidden prefix followed by `data` (at least 7 bytes).
fn prefixed_sha256(data: &[u8]) -> [u8; 32] {
    // First block: prefix, then data[..7] in words 14 (3 bytes) and 15.
    let mut w = [0u32; 64];
    w[14] = u32::from_be_bytes([0, data[0], data[1], data[2]]);
    w[15] = u32::from_be_bytes([data[3], data[4], data[5], data[6]]);
    for i in 16..30 {
        let mut v = PREFIX_SCHEDULE[i - 16];
        if i - 2 >= 14 {
            v = v.wrapping_add(sigma1(w[i - 2]) ^ if i == 16 { PREFIX_SIGMA1 } else { 0 });
        }
        if i - 7 >= 14 {
            v = v.wrapping_add(w[i - 7]);
        }
        if i - 15 >= 14 {
            v = v.wrapping_add(sigma0(w[i - 15]) ^ if i == 29 { PREFIX_SIGMA0 } else { 0 });
        }
        w[i] = v;
    }
    w[30] = sigma1(w[28])
        .wrapping_add(w[23])
        .wrapping_add(sigma0(w[15]))
        .wrapping_add(w[14])
        .wrapping_add(PREFIX_K14)
        .wrapping_sub(K[14]);
    expand(&mut w, 31);
    let mut k = K;
    k[14] = PREFIX_K14;
    let mut state = add(SHA256_IV, rounds(PREFIX_STATE, &w, &k, 14));

    // The rest is plain SHA-256.
    let len = 57 + data.len() as u64;
    let mut tail = data[7..].to_vec();
    tail.push(0x80);
    tail.resize(tail.len() + (55u64.wrapping_sub(len) % 64) as usize, 0);
    tail.extend_from_slice(&(len * 8).to_be_bytes());
    for block in tail.chunks(64) {
        let mut w = [0u32; 64];
        for (i, b) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        expand(&mut w, 16);
        state = add(state, rounds(state, &w, &K, 0));
    }
    let mut out = [0u8; 32];
    for (o, s) in out.chunks_mut(4).zip(state) {
        o.copy_from_slice(&s.to_be_bytes());
    }
    out
}

fn rol16(x: u32) -> u32 {
    x.rotate_left(16)
}

/// 64-bit checksum keyed by the SHA-256 digest.
fn cs64(data: &[u8], digest: &[u8; 32]) -> [u32; 2] {
    let key_a = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) | 1;
    let key_b = u32::from_le_bytes([digest[4], digest[5], digest[6], digest[7]]) | 1;
    let factors = [key_a.wrapping_mul(0x82893933), key_b.wrapping_mul(0x82893933)];
    let mut terms: Vec<u32> =
        CS64_COEFFICIENTS.iter().enumerate().map(|(i, c)| c.0.wrapping_mul(factors[i % 2])).collect();
    let mut offsets: Vec<u32> = CS64_COEFFICIENTS.iter().map(|c| c.1).collect();

    // Little-endian words of a zero byte plus data, padded to an even count.
    let mut bytes = vec![0u8];
    bytes.extend_from_slice(data);
    bytes.resize(bytes.len().div_ceil(8) * 8, 0);
    for (i, b) in bytes.chunks(4).enumerate() {
        let word = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let even = i % 2 == 0;
        let mut term = word.wrapping_mul(if even { key_a } else { key_b });
        let mut offset = rol16(word).wrapping_mul(if even { 0xEF0569FB } else { 0xC31713DB });
        if i == 0 {
            term = term.wrapping_add(0xF2F90B1A_u32.wrapping_mul(factors[0]));
            offset = offset.wrapping_add(0x0B1A0000);
        }
        terms.push(term);
        offsets.push(offset);
    }

    let (mut swap_acc, mut swap_last, mut state, mut acc) = (0u32, 0u32, 0u32, 0u32);
    let mul = |x: u32, m: u32| rol16(x).wrapping_mul(m);
    for i in (0..terms.len()).step_by(2) {
        let (x, y) = (terms[i], terms[i + 1]);
        let q = mul(offsets[i].wrapping_add(x), 0x76F50F34);
        swap_acc = swap_acc.wrapping_add(mul(q, 0xC3EFEA97).wrapping_add(q));
        let q = mul(offsets[i + 1].wrapping_add(y), 0x1881CE3C);
        swap_last = mul(q, 0x35BD1EC9).wrapping_add(q);
        swap_acc = swap_acc.wrapping_add(swap_last);

        state = state.wrapping_mul(key_a).wrapping_add(x);
        state = mul(mul(mul(mul(state, 0xCF98B111), 0x87085B9F), 0x12CEB96D), 0x257E1D83);
        acc = acc.wrapping_add(state);
        let t = state.wrapping_mul(key_b).wrapping_add(y);
        state = mul(mul(mul(mul(t, 0xA27416F5), 0xD38396FF), 0x7C932B89), 0xBFA49F69);
        acc = acc.wrapping_add(state);
    }
    [swap_last ^ state, swap_acc ^ acc]
}

/// `Sec-MS-GEC` for a Windows file time (100 ns ticks, 5-minute aligned).
fn gec_token(ticks: u128) -> String {
    let data = format!("{ticks}{TRUSTED_CLIENT_TOKEN}");
    let digest = prefixed_sha256(data.as_bytes());
    let cs = cs64(data.as_bytes(), &digest);
    let hex = |b: &[u8]| b.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let cs: Vec<u8> = cs.iter().flat_map(|w| w.to_le_bytes()).collect();
    format!("{}-{}", hex(&cs), hex(&digest))
}

impl EdgeTts {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, voices: Mutex::new(None), clock_skew: AtomicI64::new(0) }
    }

    fn now(&self) -> i64 {
        let local = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        local + self.clock_skew.load(Ordering::Relaxed)
    }

    /// Time-based token required by the service, see [`gec_token`].
    fn sec_ms_gec(&self) -> String {
        const WIN_EPOCH: i64 = 11_644_473_600;
        let mut secs = self.now() + WIN_EPOCH;
        secs -= secs % 300;
        gec_token(secs as u128 * 10_000_000)
    }

    fn token_query(&self) -> String {
        format!("Sec-MS-GEC={}&Sec-MS-GEC-Version=2-{CHROMIUM_FULL_VERSION}", self.sec_ms_gec())
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
        let mut retried = false;
        loop {
            match tokio::time::timeout(TIMEOUT, self.request(ssml_text, voice, rate)).await {
                Err(_) => return Err(Error::Network("timed out".into())),
                Ok(Err(tungstenite::Error::Http(resp))) if resp.status() == 403 && !retried => {
                    retried = true;
                    if !self.adjust_clock(resp.headers().get("date").and_then(|v| v.to_str().ok())) {
                        return Err(Error::Service("HTTP 403".into()));
                    }
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
    fn sha256_rounds() {
        use sha2::{Digest, Sha256};
        let mut w = [0u32; 64];
        w[0] = 0x61626380;
        w[15] = 24;
        expand(&mut w, 16);
        let state = add(SHA256_IV, rounds(SHA256_IV, &w, &K, 0));
        let ours: Vec<u8> = state.iter().flat_map(|s| s.to_be_bytes()).collect();
        assert_eq!(ours, Sha256::digest(b"abc").to_vec());
    }

    #[test]
    fn gec_token_format() {
        let token = gec_token(134_066_070_000_000_000);
        let (cs, sha) = token.split_once('-').unwrap();
        assert_eq!((cs.len(), sha.len()), (16, 64));
        assert!(token.chars().all(|c| c == '-' || c.is_ascii_digit() || c.is_ascii_uppercase()));
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
