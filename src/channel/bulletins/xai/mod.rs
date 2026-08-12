//! xAI channel — official API-key access through `https://api.x.ai`.

mod auth;

#[cfg(test)]
mod tests;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{Operation, OperationKind, Provider};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.x.ai"),
    forward_headers: &["x-grok-conv-id"],
    forward_query: &[],
};

pub struct XaiChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for XaiChannel {
    fn id(&self) -> &'static str {
        "xai"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiResponses),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            pass(CompactContent, pv(P::OpenAi)),
        ];
        routes.extend(responses_ws_to(cg(OpenAiResponses)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if !ctx.status.is_success()
            || ctx.op.operation() != Operation::ListModels
            || ctx.op.kind() != OperationKind::Provider(Provider::OpenAi)
        {
            return body;
        }
        enrich_model_list(body)
    }
}

fn enrich_model_list(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(models) = value.get_mut("data").and_then(Value::as_array_mut) else {
        return body;
    };
    let mut changed = false;
    for model in models {
        let Some(object) = model.as_object_mut() else {
            continue;
        };
        if object.get("id").and_then(Value::as_str) != Some("grok-4.6") {
            continue;
        }
        object.insert("display_name".into(), Value::String("Grok 4.6".into()));
        object.insert("context_length".into(), Value::from(500_000));
        object.insert(
            "supported_parameters".into(),
            serde_json::json!(["reasoning"]),
        );
        object.insert("thinking_supported".into(), Value::Bool(true));
        changed = true;
    }
    if !changed {
        return body;
    }
    serde_json::to_vec(&value).map(Bytes::from).unwrap_or(body)
}
