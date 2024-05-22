// SPDX-License-Identifier: Apache-2.0

//! Traits for interaction of judging service and the plugins.

#![warn(missing_docs)]

pub mod judger;
pub mod language;
pub mod resource;
pub mod service;

pub use crate::{
    judger::Judger,
    service::{Service, ServiceConfig},
};
