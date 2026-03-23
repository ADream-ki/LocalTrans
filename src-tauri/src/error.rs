use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "code", content = "detail")]
pub enum AppError {
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(String),
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
