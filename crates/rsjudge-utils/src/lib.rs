// SPDX-License-Identifier: Apache-2.0

//! A collection of utility functions for the rsjudge project.

// No `(e)print` or `(e)println` in library code.
#![warn(clippy::print_stdout, clippy::print_stderr, missing_docs)]

#[macro_use]
mod error_macros;

mod trim;

pub use trim::{trim_space, trim_space_end, trim_space_start};

pub mod command;
