// SPDX-License-Identifier: Apache-2.0

//! Abstraction for services.

use std::error::Error;
use std::future::Future;

use serde::de::DeserializeOwned;

use crate::Judger;

/// A service for calling actions on [`Judger`].
///
/// Can be a server, or a middleware fetching code from a message queue.
pub trait Service<J, C>
where
    J: Judger,
    C: ServiceConfig,
{
    /// Error type of the service.
    type Error: Error;

    /// Start the service with the given judger and config.
    fn startup(
        &mut self,
        judger: J,
        config: C,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Shutdown the service gracefully.
    fn shutdown(&mut self) -> impl Future<Output = ()> + Send;
}

/// A trait for marking the service configuration.
pub trait ServiceConfig: DeserializeOwned {}
