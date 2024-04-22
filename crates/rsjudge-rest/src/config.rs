// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct RestConfig {
    pub listen: Vec<SocketAddr>,
}
