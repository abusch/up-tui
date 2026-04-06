use reqwest::header::InvalidHeaderValue;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid API token: {0}")]
    InvalidToken(#[from] InvalidHeaderValue),

    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest_middleware::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
