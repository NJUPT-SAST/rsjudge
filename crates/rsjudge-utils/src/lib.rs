// SPDX-License-Identifier: Apache-2.0

//! A collection of utility functions for the rsjudge project.

// No `(e)print` or `(e)println` in library code.
#![warn(clippy::print_stdout, clippy::print_stderr, missing_docs)]

#[macro_use]
mod error_macros;

mod trim;

pub use trim::{trim_ascii, trim_ascii_end, trim_ascii_start};

pub mod command;
