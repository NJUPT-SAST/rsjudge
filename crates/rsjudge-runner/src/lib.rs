// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]
#![cfg_attr(feature = "setgroups", feature(setgroups))]

pub use crate::error::{Error, Result};
pub use crate::utils::cap_handle::{Cap, CapHandle};

mod config;
mod error;

mod handler;
mod macros;
mod run_as;
mod tracer;
pub mod utils;
pub use config::{
    AccessMode, Action, FileAction, FilePolicy, PathMatch, PathRule, SeccompConfig, SeccompFilter,
};
pub use handler::Handler;
pub use run_as::{DropBoundingCaps, RunAs, WithSeccomp};
pub use tracer::{TracedChild, Tracer};

pub mod user;
