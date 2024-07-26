// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "serde")]
use rsjudge_traits::ServiceConfig;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AmqpConfig {
    pub uri: String,
}

#[cfg(feature = "serde")]
impl ServiceConfig for AmqpConfig {}
