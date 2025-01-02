// SPDX-License-Identifier: Apache-2.0

//! A collection of utility functions for the rsjudge project.

// No `(e)print` or `(e)println` in library code.
#![warn(clippy::print_stdout, clippy::print_stderr, missing_docs)]

#[macro_use]
mod error_macros;

/// Returns a byte slice with trailing ASCII Space bytes removed.
///
/// ASCII Space refers to the byte `0x20`.
///
/// # Examples
///
/// ```
/// use rsjudge_utils::trim_space_end;
/// assert_eq!(trim_space_end(b"\r hello world\n "), b"\r hello world\n");
/// assert_eq!(trim_space_end(b"  "), b"");
/// assert_eq!(trim_space_end(b""), b"");
/// ```
#[inline]
#[must_use = "This function does not modify the input."]
pub const fn trim_space_end(mut bytes: &[u8]) -> &[u8] {
    // Note: A pattern matching based approach (instead of indexing) allows
    // making the function const.
    while let [rest @ .., b' '] = bytes {
        bytes = rest;
    }
    bytes
}

pub mod command;
