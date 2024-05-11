// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceLimit {
    /// CPU time limit.
    pub(crate) cpu_time_limit: Option<Duration>,
    /// Wall time limit.
    ///
    /// # Note
    ///
    /// Wall time limit may be inaccurate, due to the implementation of "wait-and-check" strategy.
    pub(crate) wall_time_limit: Option<Duration>,
    /// The memory limit **in bytes**.
    pub(crate) memory_limit: Option<u64>,
    /// Max file size limit **in bytes**.
    pub(crate) max_file_size_limit: Option<u64>,
}

impl ResourceLimit {
    /// Create a new [`ResourceLimit`] with the given limits.
    pub fn new(
        cpu_time_limit: Option<Duration>,
        wall_time_limit: Option<Duration>,
        memory_limit: Option<u64>,
        max_file_size_limit: Option<u64>,
    ) -> Self {
        Self {
            cpu_time_limit,
            wall_time_limit,
            memory_limit,
            max_file_size_limit,
        }
    }
}
