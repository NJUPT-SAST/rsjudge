// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

pub use crate::{
    cap_handle::{Cap, CapHandle},
    error::{Error, Result},
};

mod error;

mod cap_handle;
mod run_as;
pub use run_as::RunAs;

pub mod user;

mod timing;
