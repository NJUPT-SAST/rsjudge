// SPDX-License-Identifier: Apache-2.0

//! Resource limit for judging code.

use std::{num::NonZeroU64, time::Duration};

/// Resource limit for judging code.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceLimit {
    /// CPU time limit.
    cpu_time_limit: Option<Duration>,
    /// Wall time limit.
    ///
    /// # Note
    ///
    /// Wall time limit may be inaccurate, due to the implementation of
    /// "wait-and-check" strategy.
    wall_time_limit: Option<Duration>,
    /// The memory limit **in bytes**.
    memory_limit: Option<NonZeroU64>,
    /// Max file size limit **in bytes**.
    max_file_size_limit: Option<NonZeroU64>,
}

impl ResourceLimit {
    /// Create a new [`ResourceLimit`] with the given limits.
    #[must_use]
    pub fn new(
        cpu_time_limit: Option<Duration>,
        wall_time_limit: Option<Duration>,
        memory_limit: Option<NonZeroU64>,
        max_file_size_limit: Option<NonZeroU64>,
    ) -> Self {
        Self {
            cpu_time_limit,
            wall_time_limit,
            memory_limit,
            max_file_size_limit,
        }
    }

    /// Get the CPU time limit.
    #[must_use]
    pub fn cpu_time_limit(&self) -> Option<Duration> {
        self.cpu_time_limit
    }

    /// Get the wall time limit.
    #[must_use]
    pub fn wall_time_limit(&self) -> Option<Duration> {
        self.wall_time_limit
    }

    /// Get the memory limit.
    #[must_use]
    pub fn memory_limit(&self) -> Option<u64> {
        self.memory_limit.map(From::from)
    }

    /// Get the max file size limit.
    #[must_use]
    pub fn max_file_size_limit(&self) -> Option<u64> {
        self.max_file_size_limit.map(From::from)
    }

    /// Set the CPU time limit.
    pub fn set_cpu_time_limit(&mut self, cpu_time_limit: Option<Duration>) -> &mut Self {
        self.cpu_time_limit = cpu_time_limit;
        self
    }

    /// Set the wall time limit.
    pub fn set_wall_time_limit(&mut self, wall_time_limit: Option<Duration>) -> &mut Self {
        self.wall_time_limit = wall_time_limit;
        self
    }

    /// Set the memory limit.
    pub fn set_memory_limit(&mut self, memory_limit: Option<NonZeroU64>) -> &mut Self {
        self.memory_limit = memory_limit;
        self
    }

    /// Set the max file size limit.
    pub fn set_max_file_size_limit(
        &mut self,
        max_file_size_limit: Option<NonZeroU64>,
    ) -> &mut Self {
        self.max_file_size_limit = max_file_size_limit;
        self
    }
}
