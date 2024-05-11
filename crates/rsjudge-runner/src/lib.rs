// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]
#![cfg_attr(setgroups, feature(setgroups))]

pub use crate::{
    error::{Error, Result},
    utils::cap_handle::{Cap, CapHandle},
};

mod error;

mod macros;
mod run_as;
pub mod utils;
pub use run_as::RunAs;

pub mod user;
