// SPDX-License-Identifier: Apache-2.0

pub mod rusage;

use std::{future::Future, process::ExitStatus, time::Duration};

use nix::sys::resource::{Resource, setrlimit};
use rsjudge_traits::resource::ResourceLimit;
use tokio::{
    process::{Child, Command},
    time::Instant,
};

use self::rusage::{ResourceUsage, WaitForResourceUsage};
use crate::Result;

#[derive(Debug)]
pub struct CommandWithResourceLimit {
    command: Command,
    timeout: Option<Duration>,
}

impl CommandWithResourceLimit {
    /// Get a reference to the inner [`Command`].
    pub fn command(&self) -> &Command {
        &self.command
    }

    /// Get a mutable reference to the inner [`Command`].
    pub fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    /// Spawn the [`Command`] with the given resource limit.
    ///
    /// This function is synchronous and won't wait for the child to exit.
    pub fn spawn(&mut self) -> Result<ChildWithDeadline> {
        Ok(ChildWithDeadline {
            child: self.command.spawn()?,
            deadline: self.timeout.map(|timeout| Instant::now() + timeout),
        })
    }
}

/// Setting resource limits for a [`Command`].
///
/// This will take the [`Command`] by value and set the [`ResourceLimit`] for
/// it.
pub trait WithResourceLimit {
    /// Register resource limit for the command.
    ///
    /// Returns a [`CommandWithResourceLimit`] which can be spawned.
    ///
    /// You can also use [`command`][fn.command] or
    /// [`command_mut`][fn.command_mut] to get the inner [`Command`] object
    /// as needed.
    ///
    /// [fn.command]: CommandWithResourceLimit::command
    /// [fn.command_mut]: CommandWithResourceLimit::command_mut
    fn with_resource_limit(self, resource_limit: ResourceLimit) -> CommandWithResourceLimit;
    /// Spawn [`Self`] with optional resource limit.
    ///
    /// This function won't wait for the child to exit.
    /// Nor will it apply the [`ResourceLimit::wall_time_limit`] automatically.
    ///
    /// However, the wall time limit can be applied by using
    /// [`wait_for_resource_usage`].
    ///
    /// This function is synchronous.
    ///
    /// # Errors
    ///
    /// This function will return an error if the child process cannot be
    /// spawned.
    ///
    /// [`wait_for_resource_usage`]: WaitForResourceUsage::wait_for_resource_usage
    fn spawn_with_resource_limit(self, resource_limit: ResourceLimit) -> Result<ChildWithDeadline>;

    /// Run [`Self`] with given resource limit.
    fn wait_with_resource_limit(
        self,
        resource_limit: ResourceLimit,
    ) -> impl Future<Output = Result<(ExitStatus, ResourceUsage)>> + Send;
}

impl WithResourceLimit for Command {
    fn with_resource_limit(mut self, resource_info: ResourceLimit) -> CommandWithResourceLimit {
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

        CommandWithResourceLimit {
            command: self,
            timeout: resource_info.wall_time_limit(),
        }
    }

    fn spawn_with_resource_limit(self, resource_limit: ResourceLimit) -> Result<ChildWithDeadline> {
        self.with_resource_limit(resource_limit).spawn()
    }

    async fn wait_with_resource_limit(
        self,
        resource_limit: ResourceLimit,
    ) -> Result<(ExitStatus, ResourceUsage)> {
        self.spawn_with_resource_limit(resource_limit)?
            .wait_for_resource_usage()
            .await
    }
}

#[derive(Debug)]
pub struct ChildWithDeadline {
    child: Child,

    deadline: Option<Instant>,
}

impl ChildWithDeadline {
    /// Get a reference to the inner [`Child`].
    pub fn child(&self) -> &Child {
        &self.child
    }

    /// Get a mutable reference to the inner [`Child`].
    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use rsjudge_traits::resource::ResourceLimit;

    use crate::{
        Error,
        utils::resources::{WithResourceLimit as _, rusage::WaitForResourceUsage as _},
    };

    #[tokio::test]
    async fn test_wait_for_resource_usage() {
        let mut command = tokio::process::Command::new("sleep");
        command.arg("10");
        let mut child = command
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
