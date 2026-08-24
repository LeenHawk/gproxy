mod actions;
mod content;
mod message;
mod typed;

pub use actions::*;
pub use content::*;
pub use message::*;
pub use typed::*;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::{ResponseItemType, ResponseItemTypeKnown, Rest};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseItem {
    Message(ResponseMessageItem),
    Typed(Box<TypedResponseItem>),
    Unknown(Value),
}

impl<'de> Deserialize<'de> for ResponseItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let Some(type_name) = value.get("type").and_then(Value::as_str) else {
            if let Ok(message) = serde_json::from_value::<ResponseMessageItem>(value.clone()) {
                return Ok(Self::Message(message));
            }
            if let Some(item) = item_reference_without_type(&value) {
                return Ok(Self::Typed(Box::new(item)));
            }
            return Ok(Self::Unknown(value));
        };
        let item_type = serde_json::from_value::<ResponseItemType>(Value::String(type_name.into()))
            .map_err(de::Error::custom)?;
        match item_type {
            ResponseItemType::Known(ResponseItemTypeKnown::Message) => {
                serde_json::from_value(value)
                    .map(Self::Message)
                    .map_err(de::Error::custom)
            }
            ResponseItemType::Known(_) => match serde_json::from_value(value.clone()) {
                Ok(item) => Ok(Self::Typed(Box::new(item))),
                Err(_) => Ok(Self::Unknown(value)),
            },
            ResponseItemType::Unknown(_) => Ok(Self::Unknown(value)),
        }
    }
}

fn item_reference_without_type(value: &Value) -> Option<TypedResponseItem> {
    let object = value.as_object()?;
    let id = object.get("id")?.as_str()?.to_owned();
    let rest: Rest = object
        .iter()
        .filter(|(key, _)| key.as_str() != "id")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    Some(TypedResponseItem::ItemReference { id, rest })
}

pub type ResponseOutputItem = ResponseItem;
