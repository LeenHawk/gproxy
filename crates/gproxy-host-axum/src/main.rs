#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::hint::black_box(gproxy_host_axum::UPDATE_SIGNING_PUBLIC_KEY);
    let config = gproxy_app::Config::from_env()?;
    let address = config.listen_addr();
    let app = gproxy_app::App::start(config).await?;
    let server = gproxy_host_axum::AxumServer::bind(app, address).await?;
    tokio::signal::ctrl_c().await?;
    server.shutdown().await?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
