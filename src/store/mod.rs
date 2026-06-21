//! Storage layer. Cache backends live under [`cache`];
//! durable persistence backends under [`persistence`].
//! Edge (wasm32) targets also expose [`libsql`] (Hrana HTTP client).

pub mod cache;
pub mod persistence;

#[cfg(target_arch = "wasm32")]
pub mod libsql;
