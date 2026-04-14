//! Provider prefix utilities — re-exported from gproxy-routing.

pub use gproxy_engine::routing::provider_prefix::{
    add_provider_prefix, split_provider_prefixed_model, strip_provider_from_body,
    strip_provider_from_uri_path,
};
