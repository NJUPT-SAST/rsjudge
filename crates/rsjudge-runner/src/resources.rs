// SPDX-License-Identifier: Apache-2.0

use std::{
    process::Output,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nix::sys::resource::{setrlimit, Resource, Usage};
use tokio::{process::Command, time::sleep};

pub struct ResourceLimit {
    /// CPU time limit.
    cpu_time_limit: Option<Duration>,
    /// Wall time limit.
    ///
    /// # Note
    ///
    /// Wall time limit may be inaccurate, due to the implementation of "wait-and-check" strategy.
    wall_time_limit: Option<Duration>,
    /// The memory limit **in bytes**.
    memory_limit: Option<u64>,
}

#[async_trait]
pub trait RunWithResourceLimit {
    /// Run [`Self`] with optional resource limit.
    ///
    /// Before running the command, please setup the command, especially the stdio of the command.
    async fn run_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> crate::Result<(Output, Usage)>;
}

#[async_trait]
impl RunWithResourceLimit for Command {
    async fn run_with_resource_limit(
        &mut self,
        resource_info: ResourceLimit,
    ) -> crate::Result<(Output, Usage)> {
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
        let mut child = self.spawn()?;
        let start = Instant::now();
        if let Some(wall_time_limit) = resource_info.wall_time_limit {
            loop {
                let elapsed = start.elapsed();
                if elapsed >= wall_time_limit {
                    child.kill().await?;
                    break Err(crate::Error::TimeLimitExceeded {
                        cpu_time: None,
                        wall_time: Some(elapsed),
                    });
                }
                if child.try_wait()?.is_some() {
                    todo!(
                        "Wait the child with a wrapped version of wait4: {:?}",
                        child.wait().await?
                    );
                    // Double check the time usage.
                }
                sleep(Duration::from_millis(10)).await;
            }
        } else {
            todo!(
                "Wait the child with a wrapped version of wait4: {:?}",
                child.wait().await?
            );
        }
    }
}
