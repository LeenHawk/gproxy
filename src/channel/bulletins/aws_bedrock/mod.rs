//! Amazon Bedrock native APIs over bearer-token authentication.

mod auth;
mod compact;
mod converse;
mod endpoint;
mod models;
mod shape;
mod stream;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common;
use crate::channel::http_util::{allow_headers, build_request};
use crate::channel::{
    Channel, ChannelError, ChannelStreamDecoder, Disposition, PrepareCtx, PreparedRequest, ShapeCtx,
};
use crate::protocol::{Operation, OperationKind, Provider};

const DEFAULT_REGION: &str = "us-east-1";
const FORWARD_HEADERS: &[&str] = &["anthropic-beta", "openai-beta"];

pub struct AwsBedrockChannel;

fn is_count_tokens(op: crate::protocol::OperationKey) -> bool {
    op.operation() == Operation::CountTokens
        && op.kind() == OperationKind::Provider(Provider::Claude)
}

fn is_models(op: crate::protocol::OperationKey) -> bool {
    matches!(op.operation(), Operation::ListModels | Operation::GetModel)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for AwsBedrockChannel {
    fn id(&self) -> &'static str {
        "aws-bedrock"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Claude)),
            pass(CountTokens, pv(P::Claude)),
            xform(CountTokens, pv(P::Gemini), CountTokens, pv(P::Claude)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            // Nova Reel is an asynchronous Bedrock Runtime operation.
            pass(CreateVideo, pv(P::OpenAi)),
            pass(RetrieveVideo, pv(P::OpenAi)),
        ];
        routes.extend(responses_ws_to(cg(ClaudeMessages)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let api_key = common::resolve_api_key(&ctx)?;
        let compact = compact::is_request(&ctx.body);
        if ctx.op.operation() == Operation::CreateVideo {
            let request: serde_json::Value =
                serde_json::from_slice(&ctx.body).map_err(|error| {
                    ChannelError::Build(format!("invalid Bedrock video request: {error}"))
                })?;
            if request
                .get("modelId")
                .and_then(serde_json::Value::as_str)
                .is_none()
            {
                return Err(ChannelError::Build(
                    "Bedrock video request requires a model id".into(),
                ));
            }
            if request.get("outputDataConfig").is_none() {
                return Err(ChannelError::Build(
                    "Bedrock video generation requires provider setting video_output_s3_uri or request field output_s3_uri"
                        .into(),
                ));
            }
        }
        let uri = endpoint::resolve(&ctx, compact)?;
        let headers = allow_headers(ctx.headers, FORWARD_HEADERS);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &api_key)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if ctx.op.operation() == Operation::CreateVideo {
            shape_nova_reel_request(body, ctx.settings)
        } else {
            shape::request(body, headers, ctx)
        }
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if matches!(
            ctx.op.operation(),
            Operation::CreateVideo | Operation::RetrieveVideo
        ) && ctx.status.is_success()
        {
            shape_bedrock_video_response(body)
        } else {
            shape::response(body, ctx)
        }
    }

    fn classify(
        &self,
        status: http::StatusCode,
        headers: &HeaderMap,
        _body: &Bytes,
    ) -> Disposition {
        if status == http::StatusCode::FORBIDDEN {
            Disposition::Permanent
        } else {
            Disposition::from_http(status, headers)
        }
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(Box::new(stream::ConverseStreamDecoder::new()))
    }
}

fn shape_nova_reel_request(body: Bytes, settings: &serde_json::Value) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(input) = value.as_object_mut() else {
            return;
        };
        let model_id = input
            .remove("model")
            .and_then(|value| value.as_str().map(str::to_owned));
        let native_input = input.remove("model_input");
        let output = input.remove("outputDataConfig").or_else(|| {
            input
                .remove("output_s3_uri")
                .or_else(|| settings.get("video_output_s3_uri").cloned())
                .and_then(|value| value.as_str().map(str::to_owned))
                .map(|s3_uri| {
                    serde_json::json!({
                        "s3OutputDataConfig": { "s3Uri": s3_uri }
                    })
                })
        });
        let model_input = native_input.unwrap_or_else(|| {
            let prompt = input
                .remove("prompt")
                .unwrap_or(serde_json::Value::String(String::new()));
            let mut config = serde_json::Map::new();
            if let Some(seconds) = input.remove("seconds") {
                let seconds = seconds
                    .as_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(serde_json::Value::from)
                    .unwrap_or(seconds);
                config.insert("durationSeconds".into(), seconds);
            }
            if let Some(size) = input.remove("size") {
                config.insert("dimension".into(), size);
            }
            if let Some(seed) = input.remove("seed") {
                config.insert("seed".into(), seed);
            }
            config
                .entry("fps")
                .or_insert_with(|| serde_json::Value::from(24));
            serde_json::json!({
                "taskType": "TEXT_VIDEO",
                "textToVideoParams": { "text": prompt },
                "videoGenerationConfig": serde_json::Value::Object(config),
            })
        });
        let mut request = serde_json::Map::new();
        if let Some(model_id) = model_id {
            request.insert("modelId".into(), serde_json::Value::String(model_id));
        }
        request.insert("modelInput".into(), model_input);
        if let Some(output) = output {
            request.insert("outputDataConfig".into(), output);
        }
        *value = serde_json::Value::Object(request);
    })
}

fn shape_bedrock_video_response(body: Bytes) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let arn = value
            .get("invocationArn")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let native_status = value
            .get("status")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let s3_uri = value
            .pointer("/outputDataConfig/s3OutputDataConfig/s3Uri")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let Some(object) = value.as_object_mut() else {
            return;
        };
        if let Some(arn) = arn.as_deref() {
            object.insert(
                "id".into(),
                serde_json::Value::String(common::encode_video_task_id(arn)),
            );
        }
        let status = match native_status.as_deref() {
            Some("Completed") => "completed",
            Some("Failed") => "failed",
            Some("InProgress") => "in_progress",
            _ if native_status.is_none() => "queued",
            _ => "in_progress",
        };
        object.insert("status".into(), serde_json::Value::String(status.into()));
        if let (Some(arn), Some(s3_uri)) = (arn.as_deref(), s3_uri) {
            let invocation_id = arn.rsplit('/').next().unwrap_or(arn);
            object.insert(
                "url".into(),
                serde_json::Value::String(format!(
                    "{}/{invocation_id}/output.mp4",
                    s3_uri.trim_end_matches('/')
                )),
            );
        }
        if let Some(message) = object
            .get("failureMessage")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
        {
            object.insert(
                "error".into(),
                serde_json::json!({ "code": "generation_failed", "message": message }),
            );
        }
    })
}

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod tests;
