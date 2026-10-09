use async_trait::async_trait;
use serde_json::{json, Value};

use super::{char_count, target_code, Provider, ProviderConfig, ProviderOutput, Sink, StreamPiece};
use crate::lang;
use crate::prompt::Job;
use crate::types::Usage;
use crate::{Error, Result};

pub struct Microsoft {
    config: ProviderConfig,
    key: String,
    http: reqwest::Client,
}

impl Microsoft {
    pub fn new(config: ProviderConfig, key: String, http: reqwest::Client) -> Self {
        Self { config, key, http }
    }
}

#[async_trait]
impl Provider for Microsoft {
    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn translate(&self, job: &Job, sink: Sink<'_>) -> Result<ProviderOutput> {
        let target = target_code(job)?;
        let base = self
            .config
            .base_url()
            .unwrap_or_else(|| "https://api.cognitive.microsofttranslator.com".into());
        let mut query = vec![("api-version", "3.0".to_string()), ("to", lang::microsoft_code(target))];
        if let Some(s) = &job.source {
            query.push(("from", lang::microsoft_code(s)));
        }
        let mut rb = self
            .http
            .post(format!("{base}/translate"))
            .query(&query)
            .header("Ocp-Apim-Subscription-Key", self.key.trim())
            .json(&json!([{ "Text": job.text }]));
        if let Some(region) = self.config.region.as_deref().filter(|r| !r.trim().is_empty()) {
            rb = rb.header("Ocp-Apim-Subscription-Region", region.trim());
        }
        let resp = rb.send().await?;
        if !resp.status().is_success() {
            return Err(Error::from_response(resp).await);
        }
        let v: Value = resp.json().await?;
        let text = v
            .pointer("/0/translations/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::BadResponse(v.to_string()))?
            .to_string();
        let source = v.pointer("/0/detectedLanguage/language").and_then(Value::as_str).map(lang::normalize);
        sink(StreamPiece::Text(text.clone()));
        Ok(ProviderOutput {
            text,
            source: source.or_else(|| job.source.clone()),
            target: Some(target.to_string()),
            usage: Usage { characters: char_count(&job.text), ..Default::default() },
        })
    }
}
