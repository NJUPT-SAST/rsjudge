// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

use std::net::SocketAddr;

use tonic::transport::{Error, Server};

use crate::proto::judge_service_server::JudgeServiceServer;
use crate::server::JudgeServerImpl;

pub mod config;
mod proto;
mod server;

/// Serve the gRPC judge server on the given address.
///
/// # Errors
///
/// This will error when the server fails to start or when the address is
/// invalid.
pub async fn serve(addr: SocketAddr) -> Result<(), Error> {
    Server::builder()
        .add_service(JudgeServiceServer::new(JudgeServerImpl))
        .serve(addr)
        .await?;
    Ok(())
}
