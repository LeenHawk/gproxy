mod image;
mod video;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use http::HeaderMap;
use serde_json::{Map, Value};

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    match ctx.key.operation {
        Operation::CreateImage | Operation::EditImage => image::request(ctx, headers),
        Operation::CreateVideo => video::request(ctx),
        Operation::Rerank => with_object(ctx.body.clone(), |object| {
            if !ctx.upstream_model.is_empty() {
                object.insert("model".into(), Value::String(ctx.upstream_model.into()));
            }
            Ok(())
        }),
        _ => {
            let body = crate::shared::openai::shape_request(
                ctx.key,
                ctx.stream,
                ctx.upstream_model,
                ctx.headers,
                ctx.body,
            )?;
            compatible_request(ctx, body)
        }
    }
}

fn compatible_request(ctx: &PrepareCtx<'_>, body: Bytes) -> Result<Bytes, ChannelError> {
    let openai = matches!(
        ctx.key.kind,
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiChat | ContentGenerationKind::OpenAiResponses
        )
    );
    let claude =
        ctx.key.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages);
    let cache = openai
        && ctx
            .provider_settings
            .get("enable_openai_magic_cache")
            .and_then(Value::as_bool)
            == Some(true);
    let fallback = claude && crate::shared::claude::fallback::enabled(ctx.provider_settings);
    if !openai && !cache && !fallback {
        return Ok(body);
    }
    with_object(body, |object| {
        if object.get("service_tier").and_then(Value::as_str) == Some("fast") {
            object.insert("service_tier".into(), Value::String("priority".into()));
        }
        let mut value = Value::Object(std::mem::take(object));
        if cache {
            let kind = match ctx.key.kind {
                OperationKind::ContentGeneration(kind) => kind,
                OperationKind::Family(_) => return Ok(()),
            };
            crate::shared::openai::cache::apply(&mut value, kind);
        }
        if fallback {
            crate::shared::claude::fallback::apply_without_beta(&mut value, ctx.provider_settings);
        }
        *object = value
            .as_object_mut()
            .map(std::mem::take)
            .expect("request remained an object");
        Ok(())
    })
}

pub(super) fn response(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if !ctx.status.is_success() {
        return Ok(super::error::shape(ctx.body));
    }
    match ctx.key.operation {
        Operation::ListModels
            if ctx
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| !value.to_ascii_lowercase().contains("json")) =>
        {
            Ok(ctx.body.clone())
        }
        Operation::ListModels => super::model::shape_list(ctx.body),
        Operation::CreateVideo | Operation::RetrieveVideo => video::response(ctx.body),
        _ => Ok(ctx.body.clone()),
    }
}

fn with_object(
    body: Bytes,
    mutate: impl FnOnce(&mut Map<String, Value>) -> Result<(), ChannelError>,
) -> Result<Bytes, ChannelError> {
    let mut value = serde_json::from_slice::<Value>(&body)
        .map_err(|error| ChannelError::Prepare(format!("request body is not JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("request body must be an object".into()))?;
    mutate(object)?;
    encode(value)
}

fn encode(value: Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
