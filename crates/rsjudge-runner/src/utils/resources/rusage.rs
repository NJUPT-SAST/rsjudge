// SPDX-License-Identifier: Apache-2.0

use std::{
    future::Future, io, mem::MaybeUninit, os::unix::process::ExitStatusExt, process::ExitStatus,
    ptr::addr_of_mut, time::Duration,
};

use nix::{
    errno::Errno,
    libc::{self, rusage},
    sys::wait::WaitPidFlag,
    unistd::Pid,
};
use tokio::{
    process::Child,
    select,
    signal::unix::{signal, SignalKind},
    time::sleep_until,
};

// use tokio_util::sync::CancellationToken;
use crate::{utils::resources::ChildWithDeadline, Error, Result};

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
    /// Uses wait4(2) internally to wait for the process to exit and get the resource usage.
    ///
    /// See [`wait4`]
    fn wait_for_resource_usage(
        &mut self,
    ) -> impl Future<Output = Result<Option<(ExitStatus, ResourceUsage)>>> + Send;
}

/// A safe wrapper for the [wait4(2)] syscall.
///
/// # Errors
///
/// See manual page: [wait4(2)]
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
                addr_of_mut!(status),
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
    async fn wait_for_resource_usage(&mut self) -> Result<Option<(ExitStatus, ResourceUsage)>> {
        if let Some(pid) = self.id() {
            let pid = Pid::from_raw(pid as _);
            let mut sigchld = signal(SignalKind::child())?;
            loop {
                if let Some(status) = wait4(pid, Some(WaitPidFlag::WNOHANG))? {
                    return Ok(Some(status));
                }

                if sigchld.recv().await.is_none() {
                    Err(Errno::ECHILD)?;
                }
            }
        } else {
            let exit_status = self
                .try_wait()?
                .ok_or_else(|| io::Error::other("Exit status not available"))?;
            if exit_status.success() {
                Ok(None)
            } else {
                Err(Error::EarlyExited(exit_status))
            }
        }
    }
}

impl WaitForResourceUsage for ChildWithDeadline {
    async fn wait_for_resource_usage(&mut self) -> Result<Option<(ExitStatus, ResourceUsage)>> {
        let Some(deadline) = self.deadline else {
            return self.child.wait_for_resource_usage().await;
        };

        select! {
            res = self.child.wait_for_resource_usage() => res,
            () = sleep_until(deadline) => {
                self.child.start_kill()?;
                return Err(Error::TimeLimitExceeded(
                    #[cfg(debug_assertions)]
                    self.child.wait_for_resource_usage().await?,
                ));
            }
        }
    }
}
