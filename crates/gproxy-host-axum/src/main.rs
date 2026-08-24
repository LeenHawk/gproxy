#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "gproxy.toml".into());
    let config = gproxy_app::Config::load(path)?;
    let address = config.listen_addr();
    let app = gproxy_app::App::start(config).await?;
    let server = gproxy_host_axum::AxumServer::bind(app, address).await?;
    tokio::signal::ctrl_c().await?;
    server.shutdown().await?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
