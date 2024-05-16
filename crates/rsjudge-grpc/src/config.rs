// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

#[cfg(feature = "serde")]
use rsjudge_traits::ServiceConfig;

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct GrpcConfig {
    pub listen: Vec<SocketAddr>,
}

#[cfg(feature = "serde")]
impl ServiceConfig for GrpcConfig {}
