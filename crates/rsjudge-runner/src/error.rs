use caps::Capability;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{cap} is required to run as another user.")]
    CapsRequired { cap: Capability },
    #[error("User '{name}' not found")]
    UserNotFound { name: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    CapsError(#[from] caps::errors::CapsError),
}

pub type Result<T> = std::result::Result<T, Error>;
