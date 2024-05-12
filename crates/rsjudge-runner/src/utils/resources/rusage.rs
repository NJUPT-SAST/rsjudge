// SPDX-License-Identifier: Apache-2.0

use std::{
    mem::MaybeUninit, os::unix::process::ExitStatusExt, process::ExitStatus, time::Duration,
};

use async_trait::async_trait;
use nix::{
    errno::Errno,
    libc::{rusage, wait4},
};
use tokio::{process::Child, select, time::sleep};
use tokio_util::sync::CancellationToken;

use crate::{utils::resources::ChildWithTimeout, Error, Result};

#[derive(Debug, Clone, Copy)]
pub struct ResourceUsage(rusage);

impl AsRef<rusage> for ResourceUsage {
    fn as_ref(&self) -> &rusage {
        &self.0
    }
}

impl AsMut<rusage> for ResourceUsage {
    fn as_mut(&mut self) -> &mut rusage {
        &mut self.0
    }
}

impl From<rusage> for ResourceUsage {
    fn from(rusage: rusage) -> Self {
        Self(rusage)
    }
}

impl ResourceUsage {
    pub fn cpu_time(&self) -> Duration {
        Duration::new(
            self.0.ru_utime.tv_sec as u64,
            self.0.ru_utime.tv_usec as u32 * 1000,
        ) + Duration::new(
            self.0.ru_stime.tv_sec as u64,
            self.0.ru_stime.tv_usec as u32 * 1000,
        )
    }

    /// Get the maximum RAM usage (resident set size) in bytes.
    pub fn ram_usage(&self) -> u64 {
        self.0.ru_maxrss as u64
    }
}
#[async_trait]
pub trait WaitForResourceUsage {
    /// Wait for the resource usage of the process.
    ///
    /// Uses wait4(2) internally to wait for the process to exit and get the resource usage.
    ///
    /// See [`wait4`]
    async fn wait_for_resource_usage(&mut self) -> Result<(ExitStatus, Option<ResourceUsage>)>;
}

fn safe_wait4(pid: i32, options: i32) -> Result<(ExitStatus, ResourceUsage)> {
    dbg!();
    let mut status = MaybeUninit::uninit();
    let mut rusage = MaybeUninit::uninit();
    let value = unsafe { wait4(pid, status.as_mut_ptr(), options, rusage.as_mut_ptr()) };
    Ok(Errno::result(value).map(|_| {
        (
            ExitStatusExt::from_raw(unsafe { status.assume_init() }),
            ResourceUsage(unsafe { rusage.assume_init() }),
        )
    })?)
}

#[async_trait]
impl WaitForResourceUsage for Child {
    async fn wait_for_resource_usage(&mut self) -> Result<(ExitStatus, Option<ResourceUsage>)> {
        let Some(pid) = self.id() else {
            let exit_status = self.try_wait()?.expect("Exit status not available");
            return Ok((exit_status, None));
        };
        let (exit_status, resource_usage) =
            tokio::task::spawn_blocking(move || safe_wait4(pid as _, 0)).await??;
        Ok((exit_status, Some(resource_usage)))
    }
}

#[async_trait]
impl WaitForResourceUsage for ChildWithTimeout {
    async fn wait_for_resource_usage(&mut self) -> Result<(ExitStatus, Option<ResourceUsage>)> {
        let Some(timeout) = self.timeout else {
            return self.child.wait_for_resource_usage().await;
        };

        let cancellation_token = CancellationToken::new();
        let child_token = cancellation_token.child_token();

        let start = self.start;

        tokio::spawn(async move {
            loop {
                if timeout <= start.elapsed() {
                    cancellation_token.cancel();
                    break;
                } else {
                    sleep(Duration::from_millis(10)).await;
                }
            }
        });

        select! {
            res = self.child.wait_for_resource_usage() => return res,
            _ = child_token.cancelled() => {
                self.child.start_kill()?;
                return Err(Error::TimeLimitExceeded);
            }
        }
    }
}
