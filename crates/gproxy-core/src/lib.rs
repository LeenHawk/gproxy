//! GPROXY v3 embeddable core.
//!
//! Channels, credential lifecycle, protocol transforms, and the execution
//! pipeline, consumable as a library. Hosts (axum server, edge wasm, Tauri
//! app) and other applications embed this crate; it must never depend on an
//! HTTP server framework. Host-provided services (credential persistence,
//! cache, transport overrides) enter through traits.
//!
//! The v2 implementation this rewrite draws from lives on the `main` branch
//! (locally mirrored at `samples/gproxy-v2/`).
