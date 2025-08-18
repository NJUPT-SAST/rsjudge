// SPDX-License-Identifier: Apache-2.0

pub mod config;

use std::io;
use std::net::SocketAddr;

use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;

/// Serve the REST API at the given address.
///
/// # Errors
///
/// This will error when the server fails to start or when the address is
/// invalid.
pub async fn serve(addr: SocketAddr) -> io::Result<()> {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
