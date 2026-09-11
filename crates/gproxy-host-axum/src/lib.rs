//! Native axum host for `gproxy-app`.

#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_arch = "wasm32"))]
mod announce;
#[cfg(not(target_arch = "wasm32"))]
mod autostart;
#[cfg(not(target_arch = "wasm32"))]
mod ingress;
#[cfg(not(target_arch = "wasm32"))]
mod request_policy;
#[cfg(not(target_arch = "wasm32"))]
mod response;
#[cfg(not(target_arch = "wasm32"))]
mod selfupdate;
#[cfg(not(target_arch = "wasm32"))]
mod server;
#[cfg(not(target_arch = "wasm32"))]
mod signature;
#[cfg(not(target_arch = "wasm32"))]
mod static_assets;
#[cfg(not(target_arch = "wasm32"))]
mod websocket;
#[cfg(windows)]
mod windows_package;

#[cfg(not(target_arch = "wasm32"))]
pub use server::{AxumServer, HostConfig, HostError};

#[cfg(not(target_arch = "wasm32"))]
mod logging;
#[cfg(not(target_arch = "wasm32"))]
mod outbound;
#[cfg(not(target_arch = "wasm32"))]
pub use logging::init as init_tracing;

pub const UPDATE_SIGNING_PUBLIC_KEY: Option<&str> = option_env!("GPROXY_UPDATE_PUBKEY");
pub const BUILD_VERSION: &str = match option_env!("GPROXY_BUILD_VERSION") {
    Some(value) => value,
    None => env!("CARGO_PKG_VERSION"),
};
pub const BUILD_CHANNEL: &str = match option_env!("GPROXY_BUILD_CHANNEL") {
    Some(value) => value,
    None => "development",
};
pub const INSTALLATION_KIND: &str = match option_env!("GPROXY_INSTALLATION_KIND") {
    Some(value) => value,
    None => "source",
};
pub const BUILD_HASH: &str = match option_env!("GPROXY_BUILD_HASH") {
    Some(value) => value,
    None => "unknown",
};

pub fn installation_kind() -> &'static str {
    #[cfg(windows)]
    if windows_package::is_packaged() {
        return "microsoft-store";
    }
    INSTALLATION_KIND
}

pub fn version_line() -> String {
    format!(
        "{} (channel {}, build {}, installation {})",
        BUILD_VERSION,
        BUILD_CHANNEL,
        BUILD_HASH.get(..12).unwrap_or(BUILD_HASH),
        installation_kind()
    )
}
