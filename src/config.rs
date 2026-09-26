// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
#[cfg(feature = "amqp")]
use rsjudge_amqp::config::AmqpConfig;
#[cfg(feature = "grpc")]
use rsjudge_grpc::config::GrpcConfig;
#[cfg(feature = "rest")]
use rsjudge_rest::config::RestConfig;
use rsjudge_runner::SeccompConfig;
use rsjudge_traits::language::config::LanguageDef;
use serde::{Deserialize, Serialize};

/// Top-level configuration aggregate.
///
/// Each field is loaded from its own file inside the configuration directory:
/// - `executors`  ← `executors.toml`
/// - `services`   ← `services.toml`
/// - `seccomp`    ← `seccomp.toml`
#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct Config {
    pub executors: HashMap<String, LanguageDef>,
    pub services: Services,
    pub seccomp: SeccompConfig,
}

/// Optional service configurations.
///
/// Each service is only started when its section is present in
/// `services.toml`; a missing section deserializes to [`None`] and the
/// service is skipped.
#[derive(Debug, Default, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct Services {
    #[cfg(feature = "grpc")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc: Option<GrpcConfig>,
    #[cfg(feature = "amqp")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amqp: Option<AmqpConfig>,
    #[cfg(feature = "rest")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<RestConfig>,
}

impl Config {
    /// Load the full configuration from a directory containing
    /// `executors.toml`, `services.toml` and `seccomp.toml`.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the files cannot be read or parsed.
    pub async fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let executors_path = dir.join("executors.toml");
        let services_path = dir.join("services.toml");
        let seccomp_path = dir.join("seccomp.toml");

        let executors_bytes = tokio::fs::read(&executors_path).await.with_context(|| {
            format!(
                "Cannot read executor config at {}",
                executors_path.display()
            )
        })?;
        let services_bytes = tokio::fs::read(&services_path).await.with_context(|| {
            format!("Cannot read services config at {}", services_path.display())
        })?;
        let seccomp_bytes = tokio::fs::read(&seccomp_path)
            .await
            .with_context(|| format!("Cannot read seccomp config at {}", seccomp_path.display()))?;

        let executors: HashMap<String, LanguageDef> = toml::from_slice(&executors_bytes)
            .with_context(|| {
                format!(
                    "Failed to parse executor config at {}",
                    executors_path.display()
                )
            })?;
        let services: Services = toml::from_slice(&services_bytes).with_context(|| {
            format!(
                "Failed to parse services config at {}",
                services_path.display()
            )
        })?;
        let seccomp: SeccompConfig = toml::from_slice(&seccomp_bytes).with_context(|| {
            format!(
                "Failed to parse seccomp config at {}",
                seccomp_path.display()
            )
        })?;

        Ok(Self {
            executors,
            services,
            seccomp,
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(feature = "grpc", feature = "rest"))]
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
    use std::path::Path;

    #[cfg(feature = "amqp")]
    use rsjudge_amqp::config::AmqpConfig;
    #[cfg(feature = "grpc")]
    use rsjudge_grpc::config::GrpcConfig;
    #[cfg(feature = "rest")]
    use rsjudge_rest::config::RestConfig;

    use super::{Config, Services};

    #[test]
    fn test_config() -> anyhow::Result<()> {
        println!(
            "{}",
            toml::to_string_pretty(&Services {
                #[cfg(feature = "grpc")]
                grpc: Some(GrpcConfig {
                    listen: vec![
                        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 50051)),
                        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 50051, 0, 0))
                    ]
                }),
                #[cfg(feature = "rest")]
                rest: Some(RestConfig {
                    listen: vec![
                        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 80)),
                        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 80, 0, 0))
                    ]
                }),
                #[cfg(feature = "amqp")]
                amqp: Some(AmqpConfig {
                    uri: "amqp://user:bitnami@localhost".to_owned()
                }),
            })?
        );

        println!(
            "{}",
            toml::to_string_pretty(&Services {
                #[cfg(feature = "grpc")]
                grpc: None,
                #[cfg(feature = "rest")]
                rest: None,
                #[cfg(feature = "amqp")]
                amqp: None,
            })?
        );

        Ok(())
    }

    #[tokio::test]
    async fn parse_demo_seccomp_toml() {
        // Verify that the demo configuration parses successfully.
        Config::load_from_dir(Path::new("config-demo"))
            .await
            .expect("failed to load config-demo");
    }
}
