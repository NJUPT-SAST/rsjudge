// SPDX-License-Identifier: Apache-2.0

#[cfg(debug_assertions)]
use std::process::ExitStatus;
use std::{borrow::Cow, io, result::Result as StdResult};

use capctl::Cap;
use log::error;

#[cfg(debug_assertions)]
use crate::utils::resources::rusage::ResourceUsage;

/// Capabilities required but not set.
#[derive(Debug, thiserror::Error)]
#[error("{0} required but not set.")]
pub struct CapRequiredError(pub(crate) Cap);

/// The requested user is not found.
#[derive(Debug, thiserror::Error)]
#[error("User '{username}' not found")]
pub struct UserNotFoundError {
    pub(crate) username: Cow<'static, str>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Capabilities required but not set.
    #[error(transparent)]
    CapRequired(#[from] CapRequiredError),

    /// The requested user is not found.
    #[error(transparent)]
    UserNotFound(#[from] UserNotFoundError),

    /// A wrapper for [`std::io::Error`].
    #[error(transparent)]
    Io(std::io::Error),

    #[error("Time limit exceeded")]
    TimeLimitExceeded(#[cfg(debug_assertions)] (ExitStatus, ResourceUsage)),

    #[error("Child process has already exited")]
    AlreadyExited,
}

/// Convert any error implementing [`Into`]`<`[`io::Error`]`>` into
/// [`enum@Error`].
impl<E: Into<io::Error>> From<E> for Error {
    fn from(value: E) -> Self {
        Self::Io(value.into())
    }
}

/// A specialized [`Result`] type for this crate.
///
/// See the [`enum@Error`] type for the error variants.
pub type Result<T, E = Error> = StdResult<T, E>;
