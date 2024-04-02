#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

pub mod comparer;

pub use comparer::{default_comparer::DefaultComparer, CompareResult, Comparer};
