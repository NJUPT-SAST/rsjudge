// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::Judger;

#[async_trait]
pub trait Service<J, C>
where
    J: Judger,
    C: Config,
{
    type Result;
    async fn startup(&mut self, judger: J, config: C) -> Self::Result;
    async fn shutdown(&mut self);
}

pub trait Config: DeserializeOwned {}
