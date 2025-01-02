// SPDX-License-Identifier: Apache-2.0

//! Functions to get user instances.
//!
//! All functions return a reference to a static instance of [`uzers::User`] if succeeded.

use std::sync::LazyLock;

use uzers::{get_user_by_name, User};

use crate::error::{Error, Result};

/// Generate functions to get user instances.
macro_rules! users {
    ($($vis:vis fn $id:ident() => $name:literal);* $(;)?) => {
        $(
            #[doc = concat!("Get an instance of user `", $name, "`.")]
            ///
            /// # Errors
            /// Returns an error if the user is not found.
            $vis fn $id() -> Result<&'static User> {
                static INNER: LazyLock<Option<User>> = LazyLock::new(|| get_user_by_name($name));
                INNER
                    .as_ref()
                    .ok_or_else(|| Error::UserNotFound { username: $name })
            }
        )*
    };
}

users! {
    pub fn supervisor() => "rsjudge-supervisor";
    pub fn builder() => "rsjudge-builder";
    pub fn runner() => "rsjudge-runner";
}
