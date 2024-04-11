// SPDX-License-Identifier: Apache-2.0

use caps::{errors::CapsError, Capability};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// Capabilities required but not set.
    #[error("{caps:?} required but not set.")]
    CapsRequired { caps: Box<[Capability]> },

    /// The requested user is not found.
    #[error("User '{username}' not found")]
    UserNotFound { username: &'static str },

    /// A wrapper for `std::io::Error`.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A wrapper for `caps::errors::CapsError`.
    #[error(transparent)]
    CapsError(#[from] CapsError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
