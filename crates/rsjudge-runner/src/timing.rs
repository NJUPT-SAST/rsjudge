use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use tokio::process::Command;

pub trait Timing {
    /// Set the CPU time limit in seconds.
    fn cpu_time_limit(&mut self, limit: Duration) -> &mut Self;

    /// Set the wall time limit in seconds.
    ///
    /// # TODO
    ///
    /// Refer to [uu-timeout]'s implementation.
    ///
    /// [uu-timeout]: https://github.com/uutils/coreutils/blob/e194022c1f3738ca5eda69ef75fc48f813fcdd1e/src/uu/timeout/src/timeout.rs#L290-L383
    fn wall_time_limit(&mut self, limit: Duration) -> &mut Self;
}

impl Timing for Command {
    fn cpu_time_limit(&mut self, limit: Duration) -> &mut Self {
        todo!("{:?}", limit)
    }

    fn wall_time_limit(&mut self, limit: Duration) -> &mut Self {
        static START: OnceLock<Option<Instant>> = OnceLock::new();

        unsafe {
            self.pre_exec(|| {
                START.get_or_init(|| Some(Instant::now()));
                Ok(())
            })
        };

        todo!("{:?}", limit)
    }
}
