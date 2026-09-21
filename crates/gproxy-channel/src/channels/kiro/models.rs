//! The model catalogue, as an OpenAI list.
//!
//! `AmazonCodeWhispererService.ListAvailableModels` answers `{models: [...]}`
//! where an entry is either a bare id or an object naming it under `modelId`.
//! The channel declares `Dialect::OpenAi` for `ListModels`, so the reply is
//! rewritten into `{object: "list", data: [...]}` before the host sees it
//! (v3 `kiro/model_list.rs`).

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde_json::{Map, Value, json};

pub(super) fn rewrite(body: &[u8]) -> Result<Bytes, ChannelError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| invalid(format!("model JSON: {error}")))?;
    let Value::Object(mut root) = value else {
        return Err(invalid("the model response is not an object"));
    };
    let models = root
        .remove("models")
        .or_else(|| root.remove("data"))
        .and_then(|models| match models {
            Value::Array(models) => Some(models),
            _ => None,
        })
        .ok_or_else(|| invalid("the model response has no models"))?;
    let data = models
        .into_iter()
        .filter_map(|model| match model {
            Value::String(id) => Some(json!({"id": id, "object": "model"})),
            Value::Object(mut model) => {
                let id = model_id(&model)?.to_owned();
                model.insert("id".into(), Value::String(id));
                model
                    .entry("object")
                    .or_insert_with(|| Value::String("model".into()));
                Some(Value::Object(model))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok(Bytes::from(
        json!({"object": "list", "data": data}).to_string(),
    ))
}

fn model_id(value: &Map<String, Value>) -> Option<&str> {
    ["modelId", "model_id", "id", "name"]
        .into_iter()
        .find_map(|name| value.get(name).and_then(Value::as_str))
}

fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("Kiro: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_entry_shapes_become_openai_models() {
        let body = json!({"models": [
            "claude-sonnet-4.5",
            {"modelId": "claude-opus-4.5", "description": "d"},
            42,
        ]})
        .to_string();
        let out: Value = serde_json::from_slice(&rewrite(body.as_bytes()).unwrap()).unwrap();
        assert_eq!(out["object"], "list");
        assert_eq!(
            out["data"][0],
            json!({"id": "claude-sonnet-4.5", "object": "model"})
        );
        assert_eq!(out["data"][1]["id"], "claude-opus-4.5");
        assert_eq!(out["data"][1]["description"], "d");
        assert_eq!(out["data"].as_array().unwrap().len(), 2);
        assert!(rewrite(b"{}").is_err());
    }
}
