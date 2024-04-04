#![warn(clippy::print_stdout, clippy::print_stderr)]

//! A collection of utility functions for the rsjudge project.

#![warn(missing_docs)]

#[macro_use]
mod error_macros;

pub mod trim;

/// Functions for working with `std::process::Command`.
pub mod command;
