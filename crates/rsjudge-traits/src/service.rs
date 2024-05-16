// SPDX-License-Identifier: Apache-2.0

//! Abstraction for services.

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::Judger;

/// A service for calling actions on [`Judger`].
///
/// Can be a server, or a middleware fetching code from a message queue.
#[async_trait]
pub trait Service<J, C>
where
    J: Judger,
    C: ServiceConfig,
{
    /// Error type of the service.
    type Error: std::error::Error;

    /// Start the service with the given judger and config.
    async fn startup(&mut self, judger: J, config: C) -> Result<(), Self::Error>;
    /// Shutdown the service gracefully.
    async fn shutdown(&mut self);
}

/// A trait for marking the service configuration.
pub trait ServiceConfig: DeserializeOwned {}
