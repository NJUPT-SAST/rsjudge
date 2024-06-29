// SPDX-License-Identifier: Apache-2.0

//! Macro for logging when error occurs.

/// Log an error message and return an `Err` variant of `Result`.
///
/// This macro is transparent and does not affect the return value.
/// You may still want to use `?` to propagate the error to the caller.
///
/// # Examples
///
/// ```
/// use anyhow::{anyhow, Result};
/// use rsjudge_utils::log_if_error;
///
/// let result: Result<()> = Err(anyhow!("An error"));
/// log_if_error!(result);
/// ```
#[macro_export]
macro_rules! log_if_error {
    ($expr: expr) => {
        ::std::result::Result::inspect_err($expr, |err| {
            ::log::error!("{}", err);
        })
    };
}
