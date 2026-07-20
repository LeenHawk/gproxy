use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::common::*;

mod actions;
mod content;
mod message;
mod typed;

pub use actions::*;
pub use content::*;
pub use message::*;
pub use typed::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ResponseItem {
    Message(ResponseMessageItem),
    Typed(TypedResponseItem),
    Unknown(UnknownResponseItem),
}

impl<'de> Deserialize<'de> for ResponseItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let type_name = value.get("type").and_then(Value::as_str);

        let Some(type_name) = type_name else {
            if let Ok(message) = serde_json::from_value::<ResponseMessageItem>(value.clone()) {
                return Ok(Self::Message(message));
            }

            if let Some(item_reference) = item_reference_without_type(&value) {
                return Ok(Self::Typed(item_reference));
            }

            return serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom);
        };

        let item_type =
            serde_json::from_value::<ResponseItemType>(Value::String(type_name.to_owned()))
                .map_err(de::Error::custom)?;

        match item_type {
            ResponseItemType::Known(ResponseItemTypeKnown::Message) => {
                serde_json::from_value(value)
                    .map(Self::Message)
                    .map_err(de::Error::custom)
            }
            ResponseItemType::Known(_) => serde_json::from_value(value)
                .map(Self::Typed)
                .map_err(de::Error::custom),
            ResponseItemType::Unknown(_) => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

fn item_reference_without_type(value: &Value) -> Option<TypedResponseItem> {
    let object = value.as_object()?;
    let id = object.get("id")?.as_str()?.to_owned();
    let mut extra = Extra::new();

    for (key, value) in object {
        if key != "id" {
            extra.insert(key.clone(), value.clone());
        }
    }

    Some(TypedResponseItem::ItemReference { id, extra })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ResponseOutputItem(pub ResponseItem);

impl<'de> Deserialize<'de> for ResponseOutputItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let item = ResponseItem::deserialize(deserializer)?;
        validate_response_output_item(&item).map_err(de::Error::custom)?;
        Ok(Self(item))
    }
}

fn validate_response_output_item(item: &ResponseItem) -> Result<(), &'static str> {
    let ResponseItem::Typed(typed) = item else {
        return Ok(());
    };

    match typed {
        TypedResponseItem::ComputerCallOutput { id, status, .. } => {
            require_some(id, "computer_call_output.id")?;
            require_some(status, "computer_call_output.status")?;
        }
        TypedResponseItem::FunctionCallOutput { id, status, .. } => {
            require_some(id, "function_call_output.id")?;
            require_some(status, "function_call_output.status")?;
        }
        TypedResponseItem::ToolSearchCall {
            id,
            call_id,
            execution,
            status,
            ..
        } => {
            require_some(id, "tool_search_call.id")?;
            require_some(call_id, "tool_search_call.call_id")?;
            require_some(execution, "tool_search_call.execution")?;
            require_some(status, "tool_search_call.status")?;
        }
        TypedResponseItem::ToolSearchOutput {
            id,
            call_id,
            execution,
            status,
            ..
        } => {
            require_some(id, "tool_search_output.id")?;
            require_some(call_id, "tool_search_output.call_id")?;
            require_some(execution, "tool_search_output.execution")?;
            require_some(status, "tool_search_output.status")?;
        }
        TypedResponseItem::AdditionalTools { id, .. } => {
            require_some(id, "additional_tools.id")?;
        }
        TypedResponseItem::ShellCall {
            id,
            environment,
            status,
            ..
        } => {
            require_some(id, "shell_call.id")?;
            require_some(environment, "shell_call.environment")?;
            require_some(status, "shell_call.status")?;
        }
        TypedResponseItem::ShellCallOutput {
            id,
            max_output_length,
            status,
            ..
        } => {
            require_some(id, "shell_call_output.id")?;
            require_some(max_output_length, "shell_call_output.max_output_length")?;
            require_some(status, "shell_call_output.status")?;
        }
        _ => {}
    }

    Ok(())
}

fn require_some<T>(value: &Option<T>, field: &'static str) -> Result<(), &'static str> {
    value.as_ref().map(|_| ()).ok_or(field)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownResponseItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseItemType>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: `ResponseItem`/`ResponseMessageItem` must serialize flat,
    /// matching their hand-written flat `Deserialize` implementations.
    #[test]
    fn input_message_serializes_flat() {
        let flat = serde_json::json!({"type": "message", "role": "user", "content": "hi"});
        let item: ResponseItem = serde_json::from_value(flat.clone()).unwrap();
        let back = serde_json::to_value(&item).unwrap();
        assert!(
            back.get("Message").is_none() && back.get("EasyInput").is_none(),
            "must not be externally tagged: {back}"
        );
        assert_eq!(back["role"], "user", "{back}");
        assert_eq!(back, flat);
    }
}
