use async_trait::async_trait;
use serde_json::{json, Value};

use super::{char_count, target_code, Provider, ProviderConfig, ProviderOutput, Sink, StreamPiece};
use crate::lang;
use crate::prompt::Job;
use crate::types::{Formality, Usage};
use crate::{Error, Result};

pub struct Deepl {
    config: ProviderConfig,
    key: String,
    http: reqwest::Client,
}

impl Deepl {
    pub fn new(config: ProviderConfig, key: String, http: reqwest::Client) -> Self {
        Self { config, key, http }
    }

    fn endpoint(&self) -> String {
        if let Some(base) = self.config.base_url() {
            return format!("{base}/v2/translate");
        }
        if self.key.trim().ends_with(":fx") {
            "https://api-free.deepl.com/v2/translate".into()
        } else {
            "https://api.deepl.com/v2/translate".into()
        }
    }
}

#[async_trait]
impl Provider for Deepl {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn translate(&self, job: &Job, sink: Sink<'_>) -> Result<ProviderOutput> {
        let target = target_code(job)?;
        let mut body = json!({
            "text": [job.text],
            "target_lang": lang::deepl_target(target),
            "preserve_formatting": true,
        });
        if let Some(s) = &job.source {
            body["source_lang"] = json!(lang::deepl_source(s));
        }
        if let Some(ctx) = job.context.as_deref().filter(|c| !c.trim().is_empty()) {
            body["context"] = json!(ctx);
        }
        match job.formality {
            Formality::Formal => body["formality"] = json!("prefer_more"),
            Formality::Informal => body["formality"] = json!("prefer_less"),
            Formality::Default => {}
        }

        let resp = self
            .http
            .post(self.endpoint())
            .header("Authorization", format!("DeepL-Auth-Key {}", self.key.trim()))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::from_response(resp).await);
        }
        let v: Value = resp.json().await?;
        let tr = v.pointer("/translations/0").ok_or_else(|| Error::BadResponse(v.to_string()))?;
        let text = tr.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
        let source = tr.get("detected_source_language").and_then(Value::as_str).map(lang::normalize);
        sink(StreamPiece::Text(text.clone()));
        Ok(ProviderOutput {
            text,
            source: source.or_else(|| job.source.clone()),
            target: Some(target.to_string()),
            usage: Usage { characters: char_count(&job.text), ..Default::default() },
        })
    }
}
