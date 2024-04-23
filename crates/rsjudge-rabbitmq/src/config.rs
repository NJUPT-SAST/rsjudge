// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "serde")]
use rsjudge_traits::Config;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RabbitMqConfig {
    pub uri: String,
}

#[cfg(feature = "serde")]
impl Config for RabbitMqConfig {}
