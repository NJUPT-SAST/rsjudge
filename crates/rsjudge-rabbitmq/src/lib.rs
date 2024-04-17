// SPDX-License-Identifier: Apache-2.0

use futures::TryStreamExt;
use rabbitmq_stream_client::{error::ConsumerDeliveryError, Environment};
use tokio::spawn;

pub use crate::error::{Error, Result};

mod error;

pub async fn register() -> Result<()> {
    let env = Environment::builder().build().await?;
    let mut consumer = env.consumer().build("mystream").await?;

    let handle = consumer.handle();

    spawn(async move {
        while let Some(delivery) = consumer.try_next().await? {
            println!("{:?}", delivery);
        }
        Ok::<_, ConsumerDeliveryError>(())
    });

    handle.close().await?;

    Ok(())
}
