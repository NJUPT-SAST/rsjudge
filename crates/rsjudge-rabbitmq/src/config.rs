// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RabbitMqConfig {
    pub uri: String,
}
