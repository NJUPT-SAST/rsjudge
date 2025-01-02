// SPDX-License-Identifier: Apache-2.0

use std::{io, process::ExitStatus, result::Result as StdResult};

use capctl::Cap;
use log::error;
use thiserror::Error;

#[cfg(debug_assertions)]
use crate::utils::resources::rusage::ResourceUsage;

#[derive(Debug, Error)]
pub enum Error {
    /// Capabilities required but not set.
    #[error("{0} required but not set.")]
    CapRequired(Cap),

    /// The requested user is not found.
    #[error("User '{username}' not found")]
    UserNotFound { username: &'static str },

    /// A wrapper for [`std::io::Error`].
    #[error(transparent)]
    Io(std::io::Error),

    #[error("Time limit exceeded")]
    TimeLimitExceeded(#[cfg(debug_assertions)] Option<(ExitStatus, ResourceUsage)>),

    #[error("Child process has exited with status: {0:?}")]
    ChildExited(ExitStatus),
}

/// Convert any error implementing [`Into`]`<`[`io::Error`]`>` into [`Error`].
impl<E: Into<io::Error>> From<E> for Error {
    fn from(value: E) -> Self {
        Self::Io(value.into())
    }
}

pub type Result<T, E = Error> = StdResult<T, E>;
