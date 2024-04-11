// SPDX-License-Identifier: Apache-2.0

use std::{io, net::SocketAddr};

use axum::{routing::get, Router};
use tokio::net::TcpListener;

pub async fn serve(addr: SocketAddr) -> io::Result<()> {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
