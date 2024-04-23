// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use rsjudge_traits::Config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct RestConfig {
    pub listen: Vec<SocketAddr>,
}

impl Config for RestConfig {}
