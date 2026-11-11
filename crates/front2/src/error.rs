use thiserror::Error;

#[derive(Debug, Error)]
pub enum Front2Error {
    #[error("Device shell: {0}")]
    Shell(String),

    #[error("Unsupported device state: {0}")]
    Unsupported(String),

    #[error("Verification failed: {0}")]
    Verification(String),

    #[error("Image: {0}")]
    Image(String),

    #[error("Plan: {0}")]
    Plan(String),

    #[error("Pair store: {0}")]
    Store(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Engine(#[from] ltbox_core::LtboxError),
}

pub type Result<T> = std::result::Result<T, Front2Error>;
