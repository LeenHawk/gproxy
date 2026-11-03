//! Persist `Conversation`, not transport payloads. Its schema version is
//! an enum tag. Unknown schema versions are rejected; unknown enum variants
//! inside a known schema are retained for forward-compatible readers.
pub use crate::options::ProviderOptions;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

mod model;
pub use model::*;
mod message;
pub use message::*;
mod content;
pub use content::*;
mod media;
pub use media::*;
mod tool;
pub use tool::*;
mod citation;
pub use citation::*;
mod request;
pub use request::*;
mod result;
pub use result::*;
mod warning;
pub use warning::*;
