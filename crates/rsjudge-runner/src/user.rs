// SPDX-License-Identifier: Apache-2.0

//! Functions to get user instances.
//!
//! All functions return a reference to a static instance of [`uzers::User`] if succeeded.

use std::sync::OnceLock;

use uzers::{get_user_by_name, User};

use crate::{
    error::{Error, Result},
    // #[macro_use]
    // user_macro::users,
};

users! {
    pub fn supervisor() => "rsjudge-supervisor";
    pub fn builder() => "rsjudge-builder";
    pub fn runner() => "rsjudge-runner";
}
