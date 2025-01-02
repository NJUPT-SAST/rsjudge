// SPDX-License-Identifier: Apache-2.0

pub mod rusage;

use std::{
    future::Future,
    process::ExitStatus,
    time::{Duration, Instant},
};

use nix::sys::resource::{setrlimit, Resource};
use rsjudge_traits::resource::ResourceLimit;
use tokio::process::{Child, Command};

use self::rusage::ResourceUsage;
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

pub trait RunWithResourceLimit {
    /// Spawn [`Self`] with optional resource limit.
    ///
    /// This function won't wait for the child to exit.
    /// Nor will it apply the [`ResourceLimit::wall_time_limit`] automatically.
    ///
    /// However, the wall time limit can be applied by using [`WaitForResourceUsage::wait_for_resource_usage`].
    ///
    /// This function is synchronous.
    ///
    /// # Errors
    ///
    /// This function will return an error if the child process cannot be spawned.
    fn spawn_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> Result<ChildWithTimeout>;

    /// Run [`Self`] with given resource limit.
    fn wait_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> impl Future<Output = Result<Option<(ExitStatus, ResourceUsage)>>> + Send;
}

impl RunWithResourceLimit for Command {
    fn spawn_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> Result<ChildWithTimeout> {
        if let Some(cpu_time_limit) = resource_info.cpu_time_limit() {
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
        if let Some(memory_limit) = resource_info.memory_limit() {
            let set_memory_limit = move || {
                setrlimit(Resource::RLIMIT_AS, memory_limit, memory_limit)?;

                Ok(())
            };
            unsafe {
                self.pre_exec(set_memory_limit);
            }
        }

        if let Some(max_file_size_limit) = resource_info.max_file_size_limit() {
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

        Ok(ChildWithTimeout {
            child: self.spawn()?,
            start: Instant::now(),
            timeout: resource_info.wall_time_limit(),
        })
    }

    async fn wait_with_resource_limit(
        &mut self,
        resource_limit: ResourceLimit,
    ) -> Result<Option<(ExitStatus, ResourceUsage)>> {
        self.spawn_with_resource_limit(resource_limit)?
            .wait_for_resource_usage()
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use rsjudge_traits::resource::ResourceLimit;

    use crate::{
        utils::resources::{rusage::WaitForResourceUsage as _, RunWithResourceLimit},
        Error,
    };

    #[tokio::test]
    async fn test_wait_for_resource_usage() {
        let mut child = tokio::process::Command::new("sleep")
            .arg("10")
            .spawn_with_resource_limit(ResourceLimit::new(
                Some(Duration::from_secs(1)),
                Some(Duration::from_secs_f64(1.5)),
                None,
                None,
            ))
            .unwrap();

        dbg!(&child);

        let start = Instant::now();
        let error = child.wait_for_resource_usage().await.unwrap_err();
        let elapsed = start.elapsed();

        dbg!(elapsed);

        assert!(elapsed < Duration::from_secs_f32(1.52));
        assert!(matches!(error, Error::TimeLimitExceeded(_)));
    }
}
