//! Thin passthrough to `gproxy_protocol::transform::dispatch`.
//!
//! The runtime transform dispatcher lives in `gproxy-protocol` as of the
//! SDK layer refactor Step 1 (spec:
//! docs/superpowers/specs/2026-04-13-sdk-layer-refactor-design.md). This
//! module is kept as a re-export so that existing call sites in
//! `crate::engine`, `crate::retry`, `crate::store`, `gproxy_channel::channels::*`
//! continue to resolve `crate::transform_dispatch::*` paths without change.
//!
//! It will be deleted entirely once `gproxy-provider` is dissolved in a
//! later migration step; new code should import from
//! `gproxy_protocol::transform::dispatch` directly.

pub use gproxy_protocol::transform::dispatch::*;
