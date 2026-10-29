// SPDX-License-Identifier: Apache-2.0

//! One supervised execution of a compiled program.

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rsjudge_runner::utils::resources::register_resource_limit;
use rsjudge_runner::{SeccompConfig, SupervisedExit, Tracer};
use rsjudge_traits::resource::ResourceLimit;

use crate::pipeline::error::PipelineError;
use crate::pipeline::report::SyscallCount;

/// How long the supervisor keeps polling for the exit of a killed child
/// before it gives up on the process group it no longer controls.
const KILL_GRACE: Duration = Duration::from_secs(5);

/// How long the supervisor sleeps between non-blocking waits.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// One execution of a program under the sandbox.
#[derive(Debug)]
pub struct RunSpec {
    /// The program to execute.
    pub program: PathBuf,

    /// The arguments to pass to the program.
    pub args: Vec<String>,

    /// The file to connect to standard input.
    pub stdin: Option<PathBuf>,

    /// The file that receives standard output.
    pub stdout: PathBuf,

    /// The file that receives standard error.
    pub stderr: PathBuf,

    /// The working directory of the program.
    pub working_dir: PathBuf,

    /// The limits to set on the program.
    pub limits: ResourceLimit,

    /// The seccomp profile to run under.
    pub seccomp: SeccompConfig,

    /// The total elapsed-time limit of the run.
    pub elapsed_limit: Duration,
}

/// What one execution produced.
#[derive(Debug)]
pub struct RunOutput {
    /// The exit status of the program and the resources it used.
    pub exit: SupervisedExit,

    /// Whether the supervisor stopped the program at the elapsed-time limit.
    pub timed_out: bool,

    /// How long the run took, as the supervisor measured it.
    pub elapsed: Duration,

    /// What the supervisor did with each traced syscall.
    pub syscalls: Vec<SyscallCount>,
}

/// Run `spec` to completion under the sandbox.
///
/// The call blocks: the supervisor serves the ptrace events of the child on
/// the thread that spawned it, so it must not run on an async executor
/// thread. The caller moves it to a blocking task.
///
/// The child is polled with a non-blocking wait instead of a blocking one so
/// the supervisor can enforce the elapsed-time limit on the same thread that
/// owns the ptrace relationship.
///
/// # Errors
///
/// Returns an error when the profile carries no file policy, when the child
/// cannot be spawned, or when the supervisor fails to serve its events.
pub fn run(spec: RunSpec) -> Result<RunOutput, PipelineError> {
    let tracer = Tracer::new(&spec.seccomp)?.ok_or(PipelineError::NoFilePolicy)?;

    let stdin = match &spec.stdin {
        Some(path) => Stdio::from(File::open(path)?),
        None => Stdio::null(),
    };

    let mut cmd = StdCommand::new(&spec.program);
    cmd.args(&spec.args)
        .current_dir(&spec.working_dir)
        .stdin(stdin)
        .stdout(Stdio::from(File::create(&spec.stdout)?))
        .stderr(Stdio::from(File::create(&spec.stderr)?));

    // The rlimits go in before `spawn` registers the seccomp filter, so the
    // `setrlimit` calls run while the child is still unconstrained.
    register_resource_limit(&mut cmd, &spec.limits);

    let mut child = tracer.spawn(&mut cmd)?;
    let start = Instant::now();
    let mut timed_out = false;
    let mut killed_at: Option<Instant> = None;

    let exit = loop {
        match child.try_wait_with_usage()? {
            Some(exit) => break exit,
            None => {
                if !timed_out && start.elapsed() >= spec.elapsed_limit {
                    // A refused kill means the group is already gone, so the
                    // next wait reports the exit.
                    let _ = child.kill();
                    timed_out = true;
                    killed_at = Some(Instant::now());
                }

                // The supervisor documents that a tracee which leaves the
                // process group stops being served. Refuse to spin forever on
                // a group that no longer answers.
                if let Some(killed_at) = killed_at
                    && killed_at.elapsed() >= KILL_GRACE
                {
                    return Err(PipelineError::Task(
                        "the process group did not exit after being killed".to_owned(),
                    ));
                }

                thread::sleep(POLL_INTERVAL);
            }
        }
    };

    let elapsed = start.elapsed();
    let syscalls = child
        .syscall_stats()
        .iter()
        .map(|(name, stat)| SyscallCount {
            name: (*name).to_owned(),
            count: stat.count,
            denied: stat.denied,
            killed: stat.killed,
        })
        .collect();

    Ok(RunOutput {
        exit,
        timed_out,
        elapsed,
        syscalls,
    })
}

/// The CPU time and peak memory a run used, or zeros when the kernel
/// reported none.
#[must_use]
pub fn usage_of(output: &RunOutput) -> (u64, u64) {
    match output.exit.usage {
        Some(usage) => (
            u64::try_from(usage.cpu_time().as_millis()).unwrap_or(u64::MAX),
            usage.ram_usage(),
        ),
        None => (0, 0),
    }
}
