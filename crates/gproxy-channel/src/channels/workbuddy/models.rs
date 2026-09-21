//! The model catalogue, as an OpenAI list.
//!
//! `GET /v3/config` answers the plugin's whole configuration inside the
//! gateway envelope; the models are `data.models[]`, each already carrying an
//! `id`. The channel declares `Dialect::OpenAi` for `ListModels`, so the reply
//! is rewritten into `{object: "list", data: [...]}` (v3 `shape.rs`).

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde_json::{Value, json};

pub(super) fn rewrite(body: &[u8]) -> Result<Bytes, ChannelError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| invalid(format!("model JSON: {error}")))?;
    let Value::Object(mut outer) = value else {
        return Err(invalid("the model response is not an object"));
    };
    if outer.get("code").and_then(Value::as_i64) != Some(super::envelope::OK) {
        return Err(invalid(format!(
            "the model response reports {}",
            outer
                .get("code")
                .map(ToString::to_string)
                .unwrap_or_else(|| "no code".into())
        )));
    }
    let Some(Value::Object(mut inner)) = outer.remove("data") else {
        return Err(invalid("the model response has no data object"));
    };
    let Some(Value::Array(models)) = inner.remove("models") else {
        return Err(invalid("the model response has no models"));
    };
    let data = models
        .into_iter()
        .filter_map(|model| {
            let Value::Object(mut model) = model else {
                return None;
            };
            model.get("id")?.as_str()?;
            model
                .entry("object")
                .or_insert_with(|| Value::String("model".into()));
            Some(Value::Object(model))
        })
        .collect::<Vec<_>>();
    Ok(Bytes::from(
        json!({"object": "list", "data": data}).to_string(),
    ))
}

fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("WorkBuddy: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_configuration_yields_the_models_it_names() {
        let body = json!({"code": 0, "msg": "ok", "data": {
            "models": [
                {"id": "hunyuan-turbo", "label": "Hunyuan"},
                {"label": "nameless"},
                "not an object",
            ],
            "features": {"agent": true},
        }})
        .to_string();
        let out: Value = serde_json::from_slice(&rewrite(body.as_bytes()).unwrap()).unwrap();
        assert_eq!(out["object"], "list");
        assert_eq!(out["data"].as_array().unwrap().len(), 1);
        assert_eq!(out["data"][0]["id"], "hunyuan-turbo");
        assert_eq!(out["data"][0]["object"], "model");
        assert_eq!(out["data"][0]["label"], "Hunyuan");
    }

    #[test]
    fn an_envelope_that_failed_is_not_a_catalogue() {
        assert!(rewrite(json!({"code": 40001, "msg": "no"}).to_string().as_bytes()).is_err());
        assert!(rewrite(json!({"code": 0, "data": {}}).to_string().as_bytes()).is_err());
    }
}
