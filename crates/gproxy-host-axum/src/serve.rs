//! The accept loop, for the native hosts.
//!
//! `axum::serve` builds its hyper connections without a timer, and hyper's
//! header read timeout does nothing without one: a client that opens a
//! connection and sends a header a byte a minute holds it forever. This loop
//! is `axum::serve` with a timer, so the timeout is real, and otherwise the
//! same — HTTP/1 and HTTP/2 with upgrades, `ConnectInfo` for the client
//! address, and a graceful shutdown that lets in-flight requests finish.

use std::{future::Future, net::SocketAddr, time::Duration};

use axum::{Extension, Router, extract::ConnectInfo};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::{conn::auto::Builder, graceful::GracefulShutdown},
    service::TowerToHyperService,
};
use tokio::net::TcpListener;

/// How long a client may take to send a request's head.
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Serve `router` on `listener` until `shutdown` resolves, then wait for the
/// connections already open to finish.
pub async fn serve(
    listener: TcpListener,
    router: Router,
    shutdown: impl Future<Output = ()>,
) -> std::io::Result<()> {
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT);
    // Extended CONNECT, which is how a websocket rides HTTP/2.
    builder.http2().enable_connect_protocol();
    let graceful = GracefulShutdown::new();
    let mut shutdown = std::pin::pin!(shutdown);
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(accepted) => accepted,
                // A failed accept is one connection's problem (the peer went
                // away, or the process is out of descriptors for a moment),
                // not the listener's.
                Err(error) => {
                    tracing::debug!(%error, "accept failed");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
            () = &mut shutdown => break,
        };
        let service = TowerToHyperService::new(
            router
                .clone()
                .layer(Extension(ConnectInfo::<SocketAddr>(peer))),
        );
        let connection = builder
            .serve_connection_with_upgrades(TokioIo::new(stream), service)
            .into_owned();
        let connection = graceful.watch(connection);
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::trace!(%error, "connection ended with an error");
            }
        });
    }
    drop(listener);
    graceful.shutdown().await;
    // The last responses have been written; their settlements may not have.
    crate::response::settled().await;
    Ok(())
}
