use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Nothing to speak")]
    Empty,
    #[error("No voice for language “{0}”")]
    NoVoice(String),
    #[error("The offline voice is not installed")]
    NotInstalled,
    #[error("Network error: {0}")]
    Network(String),
    #[error("Speech service error: {0}")]
    Service(String),
    #[error("Offline voice error: {0}")]
    Model(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Network(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
