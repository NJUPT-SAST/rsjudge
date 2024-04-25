// SPDX-License-Identifier: Apache-2.0

use std::{io, result::Result as StdResult, time::Duration};

use capctl::Cap;
use nix::errno::Errno;
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

    #[error("Time limit exceeded: CPU time: {cpu_time:?}, wall time: {wall_time:?}")]
    TimeLimitExceeded {
        cpu_time: Option<Duration>,
        wall_time: Option<Duration>,
    },
}

/// Convert a [`capctl::Error`] to an [`Error::Io`].
impl From<capctl::Error> for Error {
    fn from(value: capctl::Error) -> Self {
        Self::Io(io::Error::from(value))
    }
}

/// Convert a [`Errno`] to an [`Error::Io`].
impl From<Errno> for Error {
    fn from(value: Errno) -> Self {
        Self::Io(io::Error::from(value))
    }
}

pub type Result<T, E = Error> = StdResult<T, E>;
