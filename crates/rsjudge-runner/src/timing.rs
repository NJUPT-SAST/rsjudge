// SPDX-License-Identifier: Apache-2.0

use std::{
    process::Output,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nix::sys::resource::{setrlimit, Resource};
use tokio::{process::Command, time::sleep};

pub struct ResourceInfo {}

#[async_trait]
pub trait Timing {
    /// Run [`Self`] with optional CPU time limit and wall time limit.
    async fn run_with_time_limit(
        &mut self,
        cpu_time_limit: Option<Duration>,
        wall_time_limit: Option<Duration>,
    ) -> crate::Result<(Output, ResourceInfo)>;
}

#[async_trait]
impl Timing for Command {
    async fn run_with_time_limit(
        &mut self,
        cpu_time_limit: Option<Duration>,
        wall_time_limit: Option<Duration>,
    ) -> crate::Result<(Output, ResourceInfo)> {
        if let Some(cpu_time_limit) = cpu_time_limit {
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
        // TODO: Trace the child process with ptrace and get the resource usage.
        let mut child = self.spawn()?;
        let start = Instant::now();
        if let Some(wall_time_limit) = wall_time_limit {
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
                    return Ok((child.wait_with_output().await?, ResourceInfo {}));
                }
                sleep(Duration::from_millis(10)).await;
            }
        } else {
            Ok((child.wait_with_output().await?, ResourceInfo {}))
        }
    }
}
