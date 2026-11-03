mod support;

use futures_util::StreamExt;

use gproxy_client_sdk::*;

use serde_json::json;

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
use support::generation::{Reply, Script, answer, early_events};
#[path = "stream/termination.rs"]
mod termination;

#[path = "stream/aggregation.rs"]
mod aggregation;

#[path = "stream/metadata.rs"]
mod metadata;

#[path = "stream/edge_cases.rs"]
mod edge_cases;
