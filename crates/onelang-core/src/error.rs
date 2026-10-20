use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("API key is missing for provider \"{0}\"")]
    MissingApiKey(String),
    #[error("Provider \"{0}\" is not configured: {1}")]
    Misconfigured(String, String),
    #[error("Network error: {0}")]
    Network(reqwest::Error),
    #[error("Provider returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("Unexpected provider response: {0}")]
    BadResponse(String),
    #[error("Mode is not supported by this provider")]
    Unsupported,
    #[error("No enabled providers")]
    NoProviders,
    /// The mode needs an LLM, and none of the enabled providers is one.
    #[error("This needs an AI model: enable an LLM provider in settings")]
    NoLlmProvider,
    #[error("Nothing to translate")]
    EmptyText,
    #[error("Cancelled")]
    Cancelled,
}

impl From<reqwest::Error> for Error {
    /// URLs are stripped: some APIs carry credentials in the query string.
    fn from(e: reqwest::Error) -> Self {
        Error::Network(e.without_url())
    }
}

impl Error {
    /// Errors after which it makes sense to try the next provider in the chain.
    pub fn is_retryable_with_other_provider(&self) -> bool {
        !matches!(self, Error::EmptyText | Error::Cancelled | Error::NoLlmProvider)
    }

    pub(crate) async fn from_response(resp: reqwest::Response) -> Error {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        let body = json_message(&body).unwrap_or(body);
        Error::Http { status, body: truncate(body.trim(), 500) }
    }
}

/// The human-readable message of a JSON error body
/// (`{"error":{"message":…}}`, `{"error":"…"}`, `{"message":…}`).
fn json_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let v = v.get("error").filter(|e| !e.is_null()).unwrap_or(&v);
    let s = v.as_str().or_else(|| ["message", "detail"].iter().find_map(|k| v.get(*k)?.as_str()))?;
    (!s.trim().is_empty()).then(|| s.to_string())
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::json_message;

    #[test]
    fn json_error_messages() {
        let groq = r#"{"error":{"message":"Rate limit reached","type":"tokens","code":"rate_limit_exceeded"}}"#;
        assert_eq!(json_message(groq).as_deref(), Some("Rate limit reached"));
        assert_eq!(json_message(r#"{"error":"Bad key"}"#).as_deref(), Some("Bad key"));
        assert_eq!(json_message(r#"{"message":"Quota exceeded"}"#).as_deref(), Some("Quota exceeded"));
        assert_eq!(json_message("<html>502</html>"), None);
        assert_eq!(json_message(r#"{"error":{"code":5}}"#), None);
    }
}
