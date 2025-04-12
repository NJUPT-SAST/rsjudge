// SPDX-License-Identifier: Apache-2.0

//! Functions to get user instances.
//!
//! All functions return a reference to a static instance of [`uzers::User`] if
//! succeeded.

use std::sync::LazyLock;

use uzers::{User, get_user_by_name};

/// Generate functions to get user instances.
macro_rules! users {
    ($($vis:vis fn $id:ident() => $name:literal);* $(;)?) => {
        $(
            #[doc = concat!("Get an instance of user `", $name, "`.")]
            ///
            /// # Errors
            /// Returns an error if the user is not found.
            $vis fn $id() -> $crate::error::Result<&'static User, $crate::error::UserNotFoundError> {
                static INNER: LazyLock<Option<User>> = LazyLock::new(|| get_user_by_name($name));
                INNER
                    .as_ref()
                    .ok_or_else(|| $crate::error::UserNotFoundError { username: $name.into() })
            }
        )*
    };
}

users! {
    pub fn supervisor() => "rsjudge-supervisor";
    pub fn builder() => "rsjudge-builder";
    pub fn runner() => "rsjudge-runner";
}
