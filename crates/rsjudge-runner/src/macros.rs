// SPDX-License-Identifier: Apache-2.0

#[macro_export]
/// Use capabilities in the current thread, in a declarative way.
///
/// Mind that this macro does not emit an expression, but a statement.
/// And it must be used inside a function returning [`rsjudge_runner::Result`][crate::Result]
/// or a compatible type.
macro_rules! use_caps {
    ($($cap:expr),* $(,)?) => {
        let handle = [$($crate::CapHandle::new($cap)?),*];
    };
}
