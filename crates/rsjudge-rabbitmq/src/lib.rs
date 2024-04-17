// SPDX-License-Identifier: Apache-2.0

use amqprs::{
    callbacks::{DefaultChannelCallback, DefaultConnectionCallback},
    connection::{Connection, OpenConnectionArguments},
};

pub use crate::error::{Error, Result};

mod error;

pub async fn register() -> Result<()> {
    // Build arguments for new connection.
    let args = OpenConnectionArguments::try_from(
        // TODO: Read from configuration file.
        "amqp://user:bitnami@localhost",
    )?;
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
