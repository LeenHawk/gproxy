mod support;
use futures_util::StreamExt;
use gproxy_client_sdk::*;
use protocol::{HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture};
use serde_json::{Value, json};
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use support::content::document;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
use support::generation::{Reply, Script, answer};

#[path = "errors_warnings/compatibility.rs"]
mod compatibility;

#[path = "errors_warnings/errors.rs"]
mod errors;

#[path = "errors_warnings/media_options.rs"]
mod media_options;
#[path = "errors_warnings/provider_options.rs"]
mod provider_options;

#[path = "errors_warnings/reasoning.rs"]
mod reasoning;
