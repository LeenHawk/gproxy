use std::collections::BTreeSet;

use gproxy_channel_api::{
    Channel, ClaudeFallbackCapabilities, PrepareCtx, claude_fallback_setting,
};
use gproxy_protocol::{ContentGenerationKind, OperationKind};
use serde_json::{Value, json};

use crate::error::CoreError;

#[derive(Clone)]
pub(super) struct Policy {
    pub capabilities: ClaudeFallbackCapabilities,
    pub candidates: Vec<Value>,
    pub default: bool,
    pub tried: BTreeSet<String>,
}

impl Policy {
    pub(super) fn read(
        channel: &dyn Channel,
        ctx: &PrepareCtx<'_>,
    ) -> Result<Option<Self>, CoreError> {
        if ctx.key.kind() != OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        {
            return Ok(None);
        }
        let Some(capabilities) = channel.claude_fallback() else {
            return Ok(None);
        };
        let body: Value = serde_json::from_slice(ctx.body)
            .map_err(|error| CoreError::Transform(error.to_string()))?;
        if gproxy_channel_api::has_fallback_credit(&body) {
            return Ok(None);
        }
        let Some(configured) = body
            .get("fallbacks")
            .filter(|value| !value.is_null())
            .cloned()
            .or_else(|| claude_fallback_setting(ctx.provider_settings))
        else {
            return Ok(None);
        };
        let mut default = configured == "default";
        let mut candidates = Vec::new();
        let mut models = BTreeSet::new();
        if let Some(entries) = configured.as_array() {
            for entry in entries {
                let model = entry
                    .as_str()
                    .or_else(|| entry.get("model").and_then(Value::as_str))
                    .ok_or_else(|| {
                        CoreError::Transform("fallback entry requires a model".into())
                    })?;
                let model = channel.fallback_model(ctx.upstream_model, model.trim());
                if model.is_empty() || model == ctx.upstream_model || !models.insert(model.clone())
                {
                    continue;
                }
                let mut entry = if entry.is_object() {
                    entry.clone()
                } else {
                    json!({})
                };
                entry["model"] = json!(model);
                candidates.push(entry);
            }
        } else if !default {
            return Err(CoreError::Transform(
                "fallbacks must be default or a model list".into(),
            ));
        }
        if candidates.len() > 3 {
            return Err(CoreError::Transform(
                "at most three fallback models are allowed".into(),
            ));
        }
        default |= candidates.is_empty();
        if candidates.is_empty()
            && let Some(recommended) = capabilities.recommended_model
        {
            let model = channel.fallback_model(ctx.upstream_model, recommended);
            if model != ctx.upstream_model {
                candidates.push(json!({"model":model}));
            }
        }
        Ok(Some(Self {
            capabilities,
            candidates,
            default,
            tried: BTreeSet::from([ctx.upstream_model.into()]),
        }))
    }

    pub(super) fn next(
        &mut self,
        channel: &dyn Channel,
        response: &Value,
        primary: &str,
    ) -> Option<Value> {
        if response["stop_reason"] != "refusal" {
            return None;
        }
        if let Some(model) = response.get("model").and_then(Value::as_str) {
            self.tried.insert(channel.fallback_model(primary, model));
        }
        if let Some(iterations) = response
            .pointer("/usage/iterations")
            .and_then(Value::as_array)
        {
            for iteration in iterations {
                if let Some(model) = iteration.get("model").and_then(Value::as_str) {
                    self.tried.insert(channel.fallback_model(primary, model));
                }
            }
        }
        if let Some(model) = response
            .pointer("/stop_details/recommended_model")
            .and_then(Value::as_str)
        {
            let model = channel.fallback_model(primary, model);
            if !self.tried.contains(&model)
                && (self.default
                    || self
                        .candidates
                        .iter()
                        .any(|candidate| candidate["model"] == model))
            {
                return Some(json!({"model":model}));
            }
        }
        self.candidates
            .iter()
            .find(|entry| {
                entry["model"]
                    .as_str()
                    .is_some_and(|model| !self.tried.contains(model))
            })
            .cloned()
    }
}
