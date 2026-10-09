use async_trait::async_trait;
use serde_json::{json, Value};

use super::{char_count, target_code, Provider, ProviderConfig, ProviderOutput, Sink, StreamPiece};
use crate::lang;
use crate::prompt::Job;
use crate::types::Usage;
use crate::{Error, Result};

/// Official Cloud Translation API v2 (API key).
pub struct GoogleCloud {
    config: ProviderConfig,
    key: String,
    http: reqwest::Client,
}

impl GoogleCloud {
    pub fn new(config: ProviderConfig, key: String, http: reqwest::Client) -> Self {
        Self { config, key, http }
    }
}

#[async_trait]
impl Provider for GoogleCloud {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn translate(&self, job: &Job, sink: Sink<'_>) -> Result<ProviderOutput> {
        let target = target_code(job)?;
        let mut body = json!({ "q": job.text, "target": lang::google_code(target), "format": "text" });
        if let Some(s) = &job.source {
            body["source"] = json!(lang::google_code(s));
        }
        let resp = self
            .http
            .post("https://translation.googleapis.com/language/translate/v2")
            .header("X-Goog-Api-Key", self.key.trim())
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::from_response(resp).await);
        }
        let v: Value = resp.json().await?;
        let tr = v.pointer("/data/translations/0").ok_or_else(|| Error::BadResponse(v.to_string()))?;
        let text = tr.get("translatedText").and_then(Value::as_str).unwrap_or_default().to_string();
        let source = tr.get("detectedSourceLanguage").and_then(Value::as_str).map(lang::normalize);
        sink(StreamPiece::Text(text.clone()));
        Ok(ProviderOutput {
            text,
            source: source.or_else(|| job.source.clone()),
            target: Some(target.to_string()),
            usage: Usage { characters: char_count(&job.text), ..Default::default() },
        })
    }
}

/// Unofficial endpoint used by browser extensions. No key, but may be rate limited.
pub struct GoogleFree {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl GoogleFree {
    pub fn new(config: ProviderConfig, http: reqwest::Client) -> Self {
        Self { config, http }
    }
}

#[async_trait]
impl Provider for GoogleFree {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn translate(&self, job: &Job, sink: Sink<'_>) -> Result<ProviderOutput> {
        let target = target_code(job)?;
        let source = job.source.as_deref().map(lang::google_code).unwrap_or_else(|| "auto".into());
        let resp = self
            .http
            .post("https://translate.googleapis.com/translate_a/single")
            .query(&[("client", "gtx"), ("sl", &source), ("tl", &lang::google_code(target)), ("dt", "t"), ("dj", "1")])
            .form(&[("q", job.text.as_str())])
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::from_response(resp).await);
        }
        let v: Value = resp.json().await?;
        let text: String = v
            .get("sentences")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| s.get("trans").and_then(Value::as_str)).collect())
            .unwrap_or_default();
        if text.is_empty() {
            return Err(Error::BadResponse(crate::error::truncate(&v.to_string(), 300)));
        }
        let detected = v.get("src").and_then(Value::as_str).map(lang::normalize);
        sink(StreamPiece::Text(text.clone()));
        Ok(ProviderOutput {
            text,
            source: detected.or_else(|| job.source.clone()),
            target: Some(target.to_string()),
            usage: Usage { characters: char_count(&job.text), ..Default::default() },
        })
    }
}
