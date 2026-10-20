use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};

use super::{CheckOutput, DictionaryOutput, Provider, ProviderConfig, ProviderKind, ProviderOutput, Sink, StreamPiece};
use crate::prompt::{self, CheckJob, HeaderEvent, HeaderParser, Job, Target};
use crate::types::{CheckIssue, DictionaryEntry, Usage};
use crate::{Error, Result};

pub struct OpenAiCompat {
    config: ProviderConfig,
    key: Option<String>,
    http: reqwest::Client,
    base: String,
    model: String,
}

impl OpenAiCompat {
    pub fn new(config: ProviderConfig, key: Option<String>, http: reqwest::Client) -> Result<Self> {
        let base = config
            .base_url()
            .ok_or_else(|| Error::Misconfigured(config.name.clone(), "base URL is empty".into()))?;
        let model = config
            .model()
            .ok_or_else(|| Error::Misconfigured(config.name.clone(), "model is empty".into()))?;
        Ok(Self { config, key, http, base, model })
    }

    fn request(&self, path: &str) -> reqwest::RequestBuilder {
        let mut rb = self.http.post(format!("{}{}", self.base, path));
        if let Some(k) = &self.key {
            rb = rb.bearer_auth(k);
        }
        if self.config.kind == ProviderKind::OpenRouter {
            rb = rb.header("HTTP-Referer", "https://github.com/q-sn/1lang").header("X-Title", "1lang");
        }
        rb
    }

    fn body(&self, system: String, user: String, stream: bool) -> Value {
        let mut body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
            "stream": stream,
            "temperature": self.config.temperature.unwrap_or(0.2),
        });
        if stream {
            body["stream_options"] = json!({ "include_usage": true });
        }
        // Reasoning models answer much faster with minimal reasoning, and the
        // reasoning text must not leak into the translation.
        if self.config.kind == ProviderKind::Groq && self.model.contains("gpt-oss") {
            body["reasoning_effort"] = json!("low");
            body["include_reasoning"] = json!(false);
        } else if self.config.kind == ProviderKind::Groq && self.model.contains("qwen3") {
            body["reasoning_effort"] = json!("none");
        }
        if let Some(extra) = self.config.extra_body.as_deref().filter(|s| !s.trim().is_empty()) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(extra) {
                for (k, v) in map {
                    body[k] = v;
                }
            } else {
                log::warn!("{}: extra_body is not a JSON object, ignored", self.config.name);
            }
        }
        body
    }

    /// A non-streamed reply that must be a JSON object.
    async fn json_reply(&self, p: prompt::Prompt, what: &str) -> Result<(Value, Usage)> {
        let prompt_len = estimate_tokens(&p.system) + estimate_tokens(&p.user);
        let mut body = self.body(p.system, p.user, false);
        if self.config.kind != ProviderKind::Anthropic {
            body["response_format"] = json!({ "type": "json_object" });
        }
        let resp = self.request("/chat/completions").json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(Error::from_response(resp).await);
        }
        let v: Value = resp.json().await?;
        let content = v
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::BadResponse("no content".into()))?;
        let json: Value = serde_json::from_str(prompt::extract_json(content))
            .map_err(|e| Error::BadResponse(format!("{what} JSON: {e}")))?;
        let usage = parse_usage(&v).unwrap_or_else(|| Usage {
            input_tokens: prompt_len,
            output_tokens: estimate_tokens(content),
            characters: 0,
        });
        Ok((json, usage))
    }
}

/// Lenient: unknown types and severities fall back to defaults rather than dropping the issue.
fn parse_issue(v: &Value) -> Option<CheckIssue> {
    let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let explanation = field("explanation");
    if explanation.is_empty() {
        return None;
    }
    let kind = serde_json::from_value(v.get("type").cloned().unwrap_or_default()).unwrap_or_default();
    let severity = serde_json::from_value(v.get("severity").cloned().unwrap_or_default()).unwrap_or_default();
    Some(CheckIssue {
        original: field("original"),
        correction: field("correction"),
        kind,
        severity,
        explanation,
        segments: vec![],
    })
}

fn parse_usage(v: &Value) -> Option<Usage> {
    let u = v.get("usage").filter(|u| !u.is_null()).or_else(|| v.pointer("/x_groq/usage"))?;
    Some(Usage {
        input_tokens: u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
        output_tokens: u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
        characters: 0,
    })
}

fn estimate_tokens(s: &str) -> u32 {
    (s.chars().count() as u32 / 4).max(1)
}

#[async_trait]
impl Provider for OpenAiCompat {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn translate(&self, job: &Job, sink: Sink<'_>) -> Result<ProviderOutput> {
        let p = prompt::translation(job);
        let prompt_len = estimate_tokens(&p.system) + estimate_tokens(&p.user);
        let resp = self.request("/chat/completions").json(&self.body(p.system, p.user, true)).send().await?;
        if !resp.status().is_success() {
            return Err(Error::from_response(resp).await);
        }

        let swap = matches!(job.target, Target::Swap { .. });
        let mut header = HeaderParser::new(swap);
        let mut out = ProviderOutput {
            source: job.source.clone(),
            target: match &job.target {
                Target::Fixed(t) => Some(t.clone()),
                Target::Swap { .. } => None,
            },
            ..Default::default()
        };
        let handle = |events: Vec<HeaderEvent>, out: &mut ProviderOutput| {
            for e in events {
                match e {
                    HeaderEvent::Direction { source, target } => {
                        out.source = Some(source.clone());
                        out.target = Some(target.clone());
                        sink(StreamPiece::Direction { source, target });
                    }
                    HeaderEvent::Text(t) => {
                        out.text.push_str(&t);
                        sink(StreamPiece::Text(t));
                    }
                }
            }
        };

        let mut stream = resp.bytes_stream();
        // Raw bytes: a multi-byte character may be split between network chunks,
        // so only complete lines are decoded.
        let mut buf: Vec<u8> = Vec::new();
        'outer: while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk?);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line = String::from_utf8_lossy(&buf[..pos]).trim().to_string();
                buf.drain(..=pos);
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" {
                    break 'outer;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                if let Some(err) = v.get("error") {
                    return Err(Error::BadResponse(err.to_string()));
                }
                if let Some(u) = parse_usage(&v) {
                    out.usage = u;
                }
                if let Some(delta) = v.pointer("/choices/0/delta/content").and_then(Value::as_str) {
                    if !delta.is_empty() {
                        handle(header.push(delta), &mut out);
                    }
                }
            }
        }
        handle(header.finish(), &mut out);

        if out.usage.input_tokens == 0 && out.usage.output_tokens == 0 {
            out.usage = Usage { input_tokens: prompt_len, output_tokens: estimate_tokens(&out.text), characters: 0 };
        }
        // Models sometimes add a trailing newline; source rarely ends with one.
        if !job.text.ends_with('\n') {
            let trimmed = out.text.trim_end().len();
            out.text.truncate(trimmed);
        }
        if out.text.trim().is_empty() {
            return Err(Error::BadResponse("empty translation".into()));
        }
        Ok(out)
    }

    async fn dictionary(&self, job: &Job) -> Result<DictionaryOutput> {
        let (json, usage) = self.json_reply(prompt::dictionary(job), "dictionary").await?;
        let entry: DictionaryEntry =
            serde_json::from_value(json.clone()).map_err(|e| Error::BadResponse(format!("dictionary JSON: {e}")))?;
        if entry.senses.is_empty() {
            return Err(Error::BadResponse("dictionary has no senses".into()));
        }
        let lang = |k: &str| json.get(k).and_then(Value::as_str).map(crate::lang::normalize);
        Ok(DictionaryOutput {
            source: lang("source_lang").or_else(|| job.source.clone()),
            target: lang("target_lang").or(match &job.target {
                Target::Fixed(t) => Some(t.clone()),
                _ => None,
            }),
            usage,
            entry,
        })
    }

    async fn check(&self, job: &CheckJob) -> Result<CheckOutput> {
        let (json, usage) = self.json_reply(prompt::check(job), "check").await?;
        let mut corrected = json
            .get("corrected")
            .and_then(Value::as_str)
            .filter(|c| !c.trim().is_empty())
            .ok_or_else(|| Error::BadResponse("check has no corrected text".into()))?
            .to_string();
        if !job.text.ends_with('\n') {
            let trimmed = corrected.trim_end().len();
            corrected.truncate(trimmed);
        }
        let issues = json
            .get("issues")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(parse_issue).collect())
            .unwrap_or_default();
        Ok(CheckOutput {
            corrected,
            lang: json.get("lang").and_then(Value::as_str).map(crate::lang::normalize),
            issues,
            usage,
        })
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let mut rb = self.http.get(format!("{}/models", self.base));
        if let Some(k) = &self.key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb.send().await?;
        if !resp.status().is_success() {
            return Err(Error::from_response(resp).await);
        }
        let v: Value = resp.json().await?;
        let mut ids: Vec<String> = v
            .get("data")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from)).collect())
            .unwrap_or_default();
        // Only chat models are useful for translation.
        const NOT_CHAT: [&str; 10] =
            ["whisper", "tts", "orpheus", "guard", "embed", "dall-e", "image", "audio", "moderation", "transcribe"];
        ids.retain(|id| {
            let l = id.to_ascii_lowercase();
            !NOT_CHAT.iter().any(|w| l.contains(w))
        });
        ids.sort();
        Ok(ids)
    }
}
