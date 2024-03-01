use tonic::{
    async_trait, codegen::tokio_stream::wrappers::ReceiverStream, Request, Response, Status,
};

use crate::proto::{
    judge_service_server::JudgeService, SelfTestRequest, SelfTestResponse, SubmitRequest,
    SubmitResponse,
};

#[derive(Debug, Default)]
pub(crate) struct JudgeServerImpl;

#[async_trait]
impl JudgeService for JudgeServerImpl {
    type SelfTestStream = ReceiverStream<Result<SelfTestResponse, Status>>;

    async fn self_test(
        &self,
        request: Request<SelfTestRequest>,
    ) -> Result<Response<Self::SelfTestStream>, Status> {
        let _ = request;
        todo!()
    }

    type SubmitStream = ReceiverStream<Result<SubmitResponse, Status>>;

    async fn submit(
        &self,
        request: Request<SubmitRequest>,
    ) -> Result<Response<Self::SubmitStream>, Status> {
        let _ = request;
        todo!()
    }
}
