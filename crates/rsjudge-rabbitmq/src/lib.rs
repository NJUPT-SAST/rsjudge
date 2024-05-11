// SPDX-License-Identifier: Apache-2.0

use amqprs::{
    callbacks::{ DefaultChannelCallback, DefaultConnectionCallback}, channel::{BasicAckArguments, BasicConsumeArguments, BasicPublishArguments, BasicQosArguments, Channel, QueueDeclareArguments}, connection::{Connection, OpenConnectionArguments}, BasicProperties, Deliver
};

use crate::config::RabbitMqConfig;
pub use crate::error::{Error, Result};

pub mod config;
mod error;

pub async fn register(config: RabbitMqConfig) -> Result<()> {
    // Build arguments for new connection.
    let conn_args = OpenConnectionArguments::try_from(&*config.uri)?;
    let connection = Connection::open(&conn_args).await?;
    connection
        .register_callback(DefaultConnectionCallback)
        .await?;

    let channel = connection.open_channel(None).await?;
    channel.register_callback(DefaultChannelCallback).await?;
    channel.flow(true).await?;
    channel.basic_qos(BasicQosArguments::default().prefetch_count(1).finish()).await?;

    let queue_name = config.queue_name;
    let queue_args = QueueDeclareArguments::new(&queue_name).durable(true).finish();
    channel.queue_declare(queue_args).await?;

    let consumer_args = BasicConsumeArguments::new("rpc_queue", "");
    let (_, mut rx) = channel.basic_consume_rx(consumer_args).await?;

    while let Some(message) = rx.recv().await {
        if let Some(payload) = message.content {
            on_request(
                &channel,
                message.deliver.unwrap(),
                message.basic_properties.unwrap(),
                payload,
            )
            .await?;
        }
    }

    // Gracefully shutdown.
    channel.close().await?;
    connection.close().await?;
    Ok(())
}

async fn on_request(channel: &Channel, method: Deliver, props: BasicProperties, payload: Vec<u8>) -> Result<()>{
    let content = std::str::from_utf8(&payload).unwrap_or("").to_string();
    let response = process(content).await.into_bytes();

    let publish_args = BasicPublishArguments::new("", props.reply_to().unwrap_or(&"".to_string()));

    let properties = BasicProperties::default()
        .with_correlation_id(props.correlation_id().unwrap_or(&"".to_string()))
        .finish();

    channel
        .basic_publish(properties, response, publish_args)
        .await?;

    channel
        .basic_ack(BasicAckArguments::new(method.delivery_tag(), false))
        .await?;

    Ok(())
}

async fn process(s: String) -> String {
    s
}
