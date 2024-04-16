// SPDX-License-Identifier: Apache-2.0

use std::io;

use capctl::Cap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// Capabilities required but not set.
    #[error("{0} required but not set.")]
    CapRequired(Cap),

    /// The requested user is not found.
    #[error("User '{username}' not found")]
    UserNotFound { username: &'static str },

    /// A wrapper for `std::io::Error`.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convert a [`capctl::Error`] to an [`Error::Io`].
impl From<capctl::Error> for Error {
    fn from(value: capctl::Error) -> Self {
        Self::Io(io::Error::from_raw_os_error(value.code()))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
