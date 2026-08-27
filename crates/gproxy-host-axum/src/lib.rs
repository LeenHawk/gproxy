//! Native axum host for `gproxy-app`.

#[cfg(not(target_arch = "wasm32"))]
mod ingress;
#[cfg(not(target_arch = "wasm32"))]
mod request_policy;
#[cfg(not(target_arch = "wasm32"))]
mod response;
#[cfg(not(target_arch = "wasm32"))]
mod server;
#[cfg(not(target_arch = "wasm32"))]
mod static_assets;
#[cfg(not(target_arch = "wasm32"))]
mod websocket;

#[cfg(not(target_arch = "wasm32"))]
pub use server::{AxumServer, HostConfig, HostError};

#[cfg(not(target_arch = "wasm32"))]
pub fn init_tracing(format: gproxy_app::LogFormat) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    match format {
        gproxy_app::LogFormat::Text => {
            let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
        }
        gproxy_app::LogFormat::Json => {
            let _ = tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .try_init();
        }
    }
}

pub const UPDATE_SIGNING_PUBLIC_KEY: Option<&str> = option_env!("GPROXY_UPDATE_PUBKEY");
