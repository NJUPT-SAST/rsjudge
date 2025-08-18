// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::time::Duration;

use nix::errno::Errno;
use nix::libc::{self, rusage};
use nix::sys::wait::WaitPidFlag;
use nix::unistd::Pid;
use tokio::process::Child;
use tokio::signal::unix::{SignalKind, signal};
use tokio::time::timeout_at;

// use tokio_util::sync::CancellationToken;
use crate::{Error, Result, utils::resources::ChildWithDeadline};

/// Resource usage of a process.
///
/// Works like [`rusage`], but smaller to reduce the size of the struct.
#[derive(Debug, Clone, Copy)]
pub struct ResourceUsage {
    cpu_time: Duration,
    /// RAM usage in *kilobytes*
    ram_usage: u64,
}

impl From<rusage> for ResourceUsage {
    fn from(rusage: rusage) -> Self {
        Self {
            cpu_time: Duration::new(
                // User time
                rusage.ru_utime.tv_sec as u64,
                rusage.ru_utime.tv_usec as u32 * 1000,
            ) + Duration::new(
                // System time
                rusage.ru_stime.tv_sec as u64,
                rusage.ru_stime.tv_usec as u32 * 1000,
            ),
            ram_usage: rusage.ru_maxrss as u64,
        }
    }
}

impl ResourceUsage {
    #[must_use]
    pub fn cpu_time(&self) -> Duration {
        self.cpu_time
    }

    /// Get the maximum RAM usage (resident set size) in *kilobytes*.
    #[must_use]
    pub fn ram_usage(&self) -> u64 {
        self.ram_usage
    }
}

pub trait WaitForResourceUsage {
    /// Wait for the resource usage of the process.
    ///
    /// Uses wait4(2) internally to wait for the process to exit and get the
    /// resource usage.
    ///
    /// See [`wait4`]
    fn wait_for_resource_usage(
        &mut self,
    ) -> impl Future<Output = Result<(ExitStatus, ResourceUsage)>> + Send;
}

/// Wait for process to change state, BSD style.
///
/// A safe wrapper for the [wait4(2)] syscall.
///
/// # Errors
///
/// See manual page: [wait4(2)]
///
/// # Note
///
/// If you have already run this function on a [`Child`]'s PID and got a result
/// of `Some`, *DO NOT RUN* `wait`, `try_wait`, etc. again on the `Child`, or
/// you will get an errno of `ECHILD`.
///
/// [wait4(2)]: https://man7.org/linux/man-pages/man2/wait4.2.html
pub fn wait4<P: Into<Option<Pid>>>(
    pid: P,
    options: Option<WaitPidFlag>,
) -> io::Result<Option<(ExitStatus, ResourceUsage)>> {
    fn wait4_inner(
        pid: Option<Pid>,
        option_bits: i32,
    ) -> io::Result<Option<(ExitStatus, ResourceUsage)>> {
        let mut status = 0;
        let mut rusage = MaybeUninit::uninit();

        let res = unsafe {
            libc::wait4(
                pid.unwrap_or_else(|| Pid::from_raw(-1)).into(),
                &raw mut status,
                option_bits,
                rusage.as_mut_ptr(),
            )
        };

        Ok(match Errno::result(res)? {
            0 => None,
            _pid => Some((
                ExitStatusExt::from_raw(status),
                ResourceUsage::from(unsafe { rusage.assume_init() }),
            )),
        })
    }

    wait4_inner(pid.into(), options.map_or(0, |bits| bits.bits()))
}

impl WaitForResourceUsage for Child {
    async fn wait_for_resource_usage(&mut self) -> Result<(ExitStatus, ResourceUsage)> {
        let pid = Pid::from_raw(self.id().ok_or(Error::AlreadyExited)? as _);
        let mut sigchld = signal(SignalKind::child())?;
        loop {
            if let Some(status) = wait4(pid, Some(WaitPidFlag::WNOHANG))? {
                return Ok(status);
            } else if sigchld.recv().await.is_none() {
                Err(Error::AlreadyExited)?;
            }
        }
    }
}

impl WaitForResourceUsage for ChildWithDeadline {
    async fn wait_for_resource_usage(&mut self) -> Result<(ExitStatus, ResourceUsage)> {
        if let Some(deadline) = self.deadline {
            match timeout_at(deadline, self.child.wait_for_resource_usage()).await {
                Ok(res) => res,
                Err(_) => {
                    self.child.start_kill()?;
                    Err(Error::TimeLimitExceeded(
                        #[cfg(debug_assertions)]
                        self.child.wait_for_resource_usage().await?,
                    ))
                }
            }
        } else {
            self.child.wait_for_resource_usage().await
        }
    }
}
