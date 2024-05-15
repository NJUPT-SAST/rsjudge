// SPDX-License-Identifier: Apache-2.0

use std::result::Result as StdResult;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("AMQP error: {0}")]
    AmqpError(#[from] amqprs::error::Error),
}

pub type Result<T, E = Error> = StdResult<T, E>;
