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
        !matches!(self, Error::EmptyText | Error::Cancelled)
    }

    pub(crate) async fn from_response(resp: reqwest::Response) -> Error {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Error::Http { status, body: truncate(&body, 500) }
    }
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
