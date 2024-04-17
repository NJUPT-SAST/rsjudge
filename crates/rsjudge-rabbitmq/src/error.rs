// SPDX-License-Identifier: Apache-2.0

use std::result::Result as StdResult;

use rabbitmq_stream_client::error::{
    ClientError, ConsumerCloseError, ConsumerCreateError, ConsumerDeliveryError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Client(#[from] ClientError),

    #[error(transparent)]
    ConsumerCreate(#[from] ConsumerCreateError),

    #[error(transparent)]
    ConsumerClose(#[from] ConsumerCloseError),

    #[error(transparent)]
    ConsumerDelivery(#[from] ConsumerDeliveryError),
}

pub type Result<T, E = Error> = StdResult<T, E>;
