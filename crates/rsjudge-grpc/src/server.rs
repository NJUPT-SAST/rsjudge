// SPDX-License-Identifier: Apache-2.0

use log::debug;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{async_trait, Request, Response, Status};

use crate::proto::{
    judge_service_server::JudgeService, SelfTestRequest, SelfTestResponse, SubmitRequest,
    SubmitResponse,
};

#[derive(Debug, Default)]
pub struct JudgeServerImpl;

#[async_trait]
impl JudgeService for JudgeServerImpl {
    type SelfTestStream = ReceiverStream<Result<SelfTestResponse, Status>>;

    async fn self_test(
        &self,
        request: Request<SelfTestRequest>,
    ) -> Result<Response<Self::SelfTestStream>, Status> {
        debug!("Received SelfTestRequest: {:?}", request.into_inner());
        Err(Status::unimplemented("Not implemented yet"))
    }

    type SubmitStream = ReceiverStream<Result<SubmitResponse, Status>>;

    async fn submit(
        &self,
        request: Request<SubmitRequest>,
    ) -> Result<Response<Self::SubmitStream>, Status> {
        debug!("Received SubmitRequest: {:?}", request.into_inner());
        Err(Status::unimplemented("Not implemented yet"))
    }
}
