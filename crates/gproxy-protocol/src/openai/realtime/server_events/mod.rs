mod events;
mod payloads;

pub use events::*;
pub use payloads::*;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeServerEvent {
    Known(Box<KnownRealtimeServerEvent>),
    Unknown(UnknownRealtimeServerEvent),
}

impl<'de> Deserialize<'de> for RealtimeServerEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if let Ok(event) = serde_json::from_value::<KnownRealtimeServerEvent>(value.clone()) {
            return Ok(Self::Known(Box::new(event)));
        }
        serde_json::from_value(value)
            .map(Self::Unknown)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UnknownRealtimeServerEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
