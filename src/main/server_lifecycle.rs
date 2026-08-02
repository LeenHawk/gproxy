use std::sync::Arc;

use gproxy::app::AppState;

pub(crate) fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json =
        std::env::var("GPROXY_LOG_FORMAT").is_ok_and(|value| value.eq_ignore_ascii_case("json"));
    // Embedders own the global subscriber when they installed one first.
    let _ = if json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .try_init()
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).try_init()
    };
}

pub(crate) async fn serve(state: AppState, bind: std::net::SocketAddr) -> anyhow::Result<()> {
    let autostart = Arc::clone(&state.autostart);
    let app = gproxy::http::server::router(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    // Register only after a successful bind, so a broken/duplicate first boot
    // cannot persist an entry that will fail on every subsequent login.
    match autostart.initialize_default() {
        Ok(status) if status.supported => tracing::info!(
            enabled = status.enabled,
            platform = status.platform,
            "automatic startup ready"
        ),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "could not initialize automatic startup"),
    }
    tracing::info!("GPROXY v2 listening on http://{bind}");
    #[cfg(windows)]
    let tray_exit = super::windows_tray::start(bind)?;
    // ConnectInfo carries the socket peer into handlers — the anchor the
    // trusted-proxy client-IP resolution verifies forwarding headers against.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(
        #[cfg(windows)]
        tray_exit,
    ))
    .await?;
    Ok(())
}

async fn shutdown_signal(#[cfg(windows)] mut tray_exit: tokio::sync::oneshot::Receiver<()>) {
    use tokio::signal;

    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => tracing::warn!("failed to install SIGTERM handler: {e}"),
        }
    };
    #[cfg(all(not(unix), not(windows)))]
    let terminate = std::future::pending::<()>();

    #[cfg(windows)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = &mut tray_exit => {},
    }
    #[cfg(not(windows))]
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    #[test]
    fn tracing_initialization_is_idempotent() {
        super::init_tracing();
        super::init_tracing();
    }
}
