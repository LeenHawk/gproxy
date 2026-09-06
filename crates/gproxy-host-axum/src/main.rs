#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::hint::black_box(gproxy_host_axum::UPDATE_SIGNING_PUBLIC_KEY);
    if version_requested() {
        println!("gproxy {}", gproxy_host_axum::version_line());
        return Ok(());
    }
    let command = gproxy_app::NativeCommand::from_env()?;
    let config = match command {
        gproxy_app::NativeCommand::Serve(config) => config,
        gproxy_app::NativeCommand::MigrateV2 { config, options } => {
            let report = gproxy_app::migrate_from_v2(&config, options).await?;
            println!("{report}");
            if report.has_blockers() {
                return Err("v2 migration was not applied; resolve the reported rows first".into());
            }
            return Ok(());
        }
    };
    let address = config.listen_addr();
    gproxy_host_axum::init_tracing(config.log_format());
    let listener = reserve_listener(address, config.restart_parent().is_some()).await?;
    let host = gproxy_host_axum::HostConfig::from_config(&config);
    let app = gproxy_app::App::start(config).await?;
    tracing::info!(instance_name = %app.instance_name(), %address, "GPROXY listening");
    let server = gproxy_host_axum::AxumServer::from_listener(app, listener, host)?;
    shutdown_signal().await?;
    server.shutdown().await?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn reserve_listener(
    address: std::net::SocketAddr,
    restarting: bool,
) -> std::io::Result<tokio::net::TcpListener> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match tokio::net::TcpListener::bind(address).await {
            Err(error)
                if restarting
                    && error.kind() == std::io::ErrorKind::AddrInUse
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            result => return result,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}

#[cfg(not(target_arch = "wasm32"))]
fn version_requested() -> bool {
    let mut args = std::env::args_os();
    let _program = args.next();
    matches!(args.next().as_deref(), Some(value) if value == "--version" || value == "-V")
        && args.next().is_none()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
