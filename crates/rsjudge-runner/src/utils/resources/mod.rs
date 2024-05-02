// SPDX-License-Identifier: Apache-2.0

pub mod rlimit;
pub mod rusage;

use std::{
    process::ExitStatus,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nix::sys::resource::{setrlimit, Resource};
use tokio::process::{Child, Command};

use self::{rlimit::ResourceLimit, rusage::ResourceUsage};
use crate::{utils::resources::rusage::WaitForResourceUsage, Result};

#[derive(Debug)]
pub struct ChildWithTimeout {
    child: Child,
    start: Instant,
    timeout: Option<Duration>,
}

impl AsRef<Child> for ChildWithTimeout {
    fn as_ref(&self) -> &Child {
        &self.child
    }
}

impl AsMut<Child> for ChildWithTimeout {
    fn as_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

#[async_trait]
pub trait RunWithResourceLimit {
    /// Spawn [`Self`] with optional resource limit.
    ///
    /// This function won't wait for the child to exit.
    /// Nor will it apply the [`ResourceLimit::wall_time_limit`] automatically.
    ///
    /// However, the wall time limit can be applied by using [`WaitForResourceUsage::wait_for_resource_usage`].
    ///
    /// This function is synchronous.
    fn spawn_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> Result<ChildWithTimeout>;

    /// Run [`Self`] with given resource limit.
    async fn wait_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> Result<(ExitStatus, Option<ResourceUsage>)>;
}

#[async_trait]
impl RunWithResourceLimit for Command {
    fn spawn_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> Result<ChildWithTimeout> {
        if let Some(cpu_time_limit) = resource_info.cpu_time_limit {
            let set_cpu_limit = move || {
                setrlimit(
                    Resource::RLIMIT_CPU,
                    cpu_time_limit.as_secs(),
                    cpu_time_limit.as_secs(),
                )?;

                Ok(())
            };
            unsafe {
                self.pre_exec(set_cpu_limit);
            }
        }
        if let Some(memory_limit) = resource_info.memory_limit {
            let set_memory_limit = move || {
                setrlimit(Resource::RLIMIT_AS, memory_limit, memory_limit)?;

                Ok(())
            };
            unsafe {
                self.pre_exec(set_memory_limit);
            }
        }

        if let Some(max_file_size_limit) = resource_info.max_file_size_limit {
            let set_max_file_size_limit = move || {
                setrlimit(
                    Resource::RLIMIT_FSIZE,
                    max_file_size_limit,
                    max_file_size_limit,
                )?;

                Ok(())
            };
            unsafe {
                self.pre_exec(set_max_file_size_limit);
            }
        }

        let child = self.spawn()?;
        Ok(ChildWithTimeout {
            child,
            start: Instant::now(),
            timeout: resource_info.wall_time_limit,
        })
    }

    async fn wait_with_resource_limit(
        &mut self,
        resource_limit: ResourceLimit,
    ) -> Result<(ExitStatus, Option<ResourceUsage>)> {
        let mut child = self.spawn_with_resource_limit(resource_limit)?;
        let (exit_status, resource_usage) = child.wait_for_resource_usage().await?;
        Ok((exit_status, resource_usage))
    }
}
