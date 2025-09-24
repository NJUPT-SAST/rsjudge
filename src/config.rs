// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

#[cfg(feature = "amqp")]
use rsjudge_amqp::config::AmqpConfig;
#[cfg(feature = "grpc")]
use rsjudge_grpc::config::GrpcConfig;
#[cfg(feature = "rest")]
use rsjudge_rest::config::RestConfig;
use rsjudge_traits::language::config::LanguageDef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct Config {
    pub executors: HashMap<String, LanguageDef>,
    pub services: Services,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct Services {
    #[cfg(feature = "grpc")]
    pub grpc: GrpcConfig,
    #[cfg(feature = "amqp")]
    pub amqp: AmqpConfig,
    #[cfg(feature = "rest")]
    pub rest: RestConfig,
}

#[cfg(test)]
mod tests {
    #[cfg(any(feature = "grpc", feature = "rest"))]
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

    #[cfg(feature = "amqp")]
    use rsjudge_amqp::config::AmqpConfig;
    #[cfg(feature = "grpc")]
    use rsjudge_grpc::config::GrpcConfig;
    #[cfg(feature = "rest")]
    use rsjudge_rest::config::RestConfig;

    use crate::config::Services;

    #[test]
    fn test_config() -> anyhow::Result<()> {
        println!(
            "{}",
            toml::to_string_pretty(&Services {
                #[cfg(feature = "grpc")]
                grpc: GrpcConfig {
                    listen: vec![
                        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 50051)),
                        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 50051, 0, 0))
                    ]
                },
                #[cfg(feature = "rest")]
                rest: RestConfig {
                    listen: vec![
                        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 80)),
                        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 80, 0, 0))
                    ]
                },
                #[cfg(feature = "amqp")]
                amqp: AmqpConfig {
                    uri: "amqp://user:bitnami@localhost".to_owned()
                },
            })?
        );
        Ok(())
    }
}
