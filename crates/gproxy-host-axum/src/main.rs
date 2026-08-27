#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::hint::black_box(gproxy_host_axum::UPDATE_SIGNING_PUBLIC_KEY);
    let config = gproxy_app::Config::from_env()?;
    let address = config.listen_addr();
    let host = gproxy_host_axum::HostConfig::from_config(&config);
    gproxy_host_axum::init_tracing(config.log_format());
    let app = gproxy_app::App::start(config).await?;
    tracing::info!(instance_name = %app.instance_name(), %address, "GPROXY listening");
    let server = gproxy_host_axum::AxumServer::bind_with_config(app, address, host).await?;
    tokio::signal::ctrl_c().await?;
    server.shutdown().await?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
