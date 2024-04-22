// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct GrpcConfig {
    pub listen: Vec<SocketAddr>,
}
