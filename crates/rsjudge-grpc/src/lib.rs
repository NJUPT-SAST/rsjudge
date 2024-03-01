use std::net::SocketAddr;

use tonic::transport::{Error, Server};

use crate::{proto::judge_service_server::JudgeServiceServer, server::JudgeServerImpl};

mod proto;
mod server;

pub async fn serve(addr: SocketAddr) -> Result<(), Error> {
    Server::builder()
        .add_service(JudgeServiceServer::new(JudgeServerImpl::default()))
        .serve(addr)
        .await?;
    Ok(())
}
