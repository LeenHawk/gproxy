//! GPROXY v2 library crate. The binary (`main.rs`) is a thin wiring
//! layer over these modules.

// Native bootstrap modules historically used the package name because they
// lived in the binary crate. Keep that path valid when compiling them into the
// reusable library entrypoint.
extern crate self as gproxy;

pub mod admin;
pub mod api;
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod autostart;
pub mod billing;
pub mod channel;
pub use gproxy_channel_api as channel_api;
pub mod config;
pub mod credentials;
pub mod crypto;
pub mod health;
pub mod http;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub mod native;
pub mod pipeline;
pub mod process;
pub use gproxy_protocol as protocol;
// Self-update is native-only: edge (wasm) deploys via the platform pipeline (§19).
#[cfg(not(target_arch = "wasm32"))]
pub mod selfupdate;
pub mod store;
pub use gproxy_tokenize as tokenize;
pub use gproxy_transform as transform;
pub mod usage;
pub mod util;

// Edge self-test exercises all edge backends; gate on the full edge bundle.
#[cfg(all(
    target_arch = "wasm32",
    feature = "persist-libsql",
    feature = "cache-libsql",
    feature = "cache-upstash"
))]
pub mod wasm_selftest;
