// SPDX-License-Identifier: Apache-2.0

use amqprs::{
    callbacks::{DefaultChannelCallback, DefaultConnectionCallback},
    connection::{Connection, OpenConnectionArguments},
};

use crate::config::RabbitMqConfig;
pub use crate::error::{Error, Result};

pub mod config;
mod error;

pub async fn register(config: RabbitMqConfig) -> Result<()> {
    // Build arguments for new connection.
    let args = OpenConnectionArguments::try_from(&*config.uri)?;
    let connection = Connection::open(&args).await?;
    connection
        .register_callback(DefaultConnectionCallback)
        .await?;
    let channel = connection.open_channel(None).await?;
    channel.register_callback(DefaultChannelCallback).await?;
    channel.flow(true).await?;

    // Gracefully shutdown.
    channel.close().await?;
    connection.close().await?;
    Ok(())
}
