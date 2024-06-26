// SPDX-License-Identifier: Apache-2.0

pub mod config;

use std::{io, net::SocketAddr};

use axum::{routing::get, Router};
use tokio::net::TcpListener;

/// Serve the REST API at the given address.
///
/// # Errors
///
pub async fn serve(addr: SocketAddr) -> io::Result<()> {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
