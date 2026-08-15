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
            pass(CreateVideo, pv(P::OpenAi)),
            pass(RetrieveVideo, pv(P::OpenAi)),
            pass(EditVideo, pv(P::OpenAi)),
            pass(ExtendVideo, pv(P::OpenAi)),
            pass(CompactContent, pv(P::OpenAi)),
        ];
        routes.extend(responses_ws_to(cg(OpenAiResponses)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let key = common::resolve_api_key(&ctx)?;
        let path = if ctx.op.operation() == Operation::CreateVideo {
            "/v1/videos/generations"
        } else {
            ctx.path
        };
        let uri = common::resolve_uri(&ctx, &DEFAULTS, path, None)?;
        let headers =
            crate::channel::http_util::allow_headers(ctx.headers, DEFAULTS.forward_headers);
        let mut req = crate::channel::http_util::build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if !matches!(
            ctx.op.operation(),
            Operation::CreateVideo | Operation::EditVideo | Operation::ExtendVideo
        ) {
            return body;
        }
        crate::channel::shaping::with_json_body(body, |value| {
            let Some(object) = value.as_object_mut() else {
                return;
            };
            if let Some(seconds) = object.remove("seconds") {
                let duration = seconds
                    .as_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Value::from)
                    .unwrap_or(seconds);
                object.entry("duration").or_insert(duration);
            }
            if ctx.op.operation() == Operation::CreateVideo {
                return;
            }
            let Some(video) = object.get_mut("video") else {
                return;
            };
            if let Some(url) = video.as_str().map(str::to_owned) {
                *video = serde_json::json!({ "url": url });
            }
        })
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if ctx.status.is_success()
            && matches!(
                ctx.op.operation(),
                Operation::CreateVideo
                    | Operation::RetrieveVideo
                    | Operation::EditVideo
                    | Operation::ExtendVideo
            )
        {
            return reshape_video_response(body);
        }
        if !ctx.status.is_success()
            || ctx.op.operation() != Operation::ListModels
            || ctx.op.kind() != OperationKind::Provider(Provider::OpenAi)
        {
            return body;
        }
        enrich_model_list(body)
    }
}

fn reshape_video_response(body: Bytes) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(object) = value.as_object_mut() else {
            return;
        };
        if object.get("id").is_none()
            && let Some(id) = object
                .get("request_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        {
            object.insert("id".into(), Value::String(id));
        }
        let status = match object.get("status").and_then(Value::as_str) {
            Some("done") => Some("completed"),
            Some("pending" | "processing") => Some("in_progress"),
            None if object.get("video").is_some() => Some("completed"),
            None => Some("queued"),
            _ => None,
        };
        if let Some(status) = status {
            object.insert("status".into(), Value::String(status.into()));
        }
        if object.get("url").is_none()
            && let Some(url) = object
                .get("video")
                .and_then(|video| video.get("url"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        {
            object.insert("url".into(), Value::String(url));
        }
    })
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
