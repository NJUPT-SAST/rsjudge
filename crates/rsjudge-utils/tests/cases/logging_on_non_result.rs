// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use rsjudge_utils::log_if_error;

fn main() {
    #[derive(Debug)]
    struct S;
    impl Display for S {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "S")
        }
    }
    impl S {
        fn map_err<F: FnOnce(Self) -> Self>(self, f: F) -> Self {
            f(self)
        }
    }

    let _ = log_if_error!(S);
}
