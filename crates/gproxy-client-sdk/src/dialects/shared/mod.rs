//! Shared typed mappings and diagnostics.
pub(crate) mod citations;
pub(crate) mod media;
pub(crate) mod reasoning;
#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
pub(crate) mod sources;
pub(crate) mod warnings;
mod wire;
use crate::{Error, Result, ir::*};
#[cfg(any(feature = "openai", feature = "chat", feature = "gemini"))]
use base64::Engine;
use gproxy_protocol::Rest;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
#[cfg(feature = "gemini")]
use std::collections::BTreeMap;
pub(crate) use wire::*;
