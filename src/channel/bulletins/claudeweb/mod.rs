//! Claude consumer-web channel — Anthropic Messages over a claude.ai browser
//! session cookie.
//!
//! A web turn is inherently multi-step: upload attachments, create a temporary
//! conversation, set its thinking mode, then stream `/completion`. The channel
//! therefore uses `PreparedRequest::CustomStream` and is native-only. Request
//! construction follows Clewdr/Clove; the stream decoder accepts both their
//! legacy `{completion}` and modern Messages-SSE response shapes.

mod auth;
mod fingerprint;
mod request;
mod sse;
mod state;
mod usage;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, Response, StatusCode, header};
use serde_json::{Value, json};

use crate::channel::{Channel, ChannelError, ChannelLogin, PrepareCtx, PreparedRequest};
use crate::http::client::{ClientError, RespStream, UpstreamClient};
use crate::protocol::Provider;

pub struct ClaudeWebChannel;

impl ClaudeWebChannel {
    pub const ID: &'static str = "claudeweb";
}

#[async_trait::async_trait]
impl Channel for ClaudeWebChannel {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn provider_family(&self) -> Provider {
        Provider::Claude
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

        let mut routes = vec![
            local(ListModels, crate::channel::routes::pv(P::Claude)),
            local(ListModels, crate::channel::routes::pv(P::OpenAi)),
            local(ListModels, crate::channel::routes::pv(P::Gemini)),
            local(GetModel, crate::channel::routes::pv(P::Claude)),
            local(GetModel, crate::channel::routes::pv(P::OpenAi)),
            local(GetModel, crate::channel::routes::pv(P::Gemini)),
            local(CountTokens, crate::channel::routes::pv(P::Claude)),
            local(CountTokens, crate::channel::routes::pv(P::OpenAi)),
            local(CountTokens, crate::channel::routes::pv(P::Gemini)),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
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
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            crate::channel::routes::pass(StreamGenerateContent, cg(ClaudeMessages)),
        ];
        routes.extend(responses_ws_to(cg(ClaudeMessages)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let base = ctx
            .provider_settings
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(auth::DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_owned();
        let prompt = ctx
            .provider_settings
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let timezone = ctx
            .provider_settings
            .get("timezone")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("UTC");
        let parsed: Value = serde_json::from_slice(&ctx.body)
            .map_err(|e| ChannelError::Build(format!("claudeweb request JSON: {e}")))?;
        let tool_results = request::tool_results(&parsed);
        if let Some(tool_use_id) = tool_results
            .first()
            .and_then(|result| result.get("tool_use_id"))
            .and_then(Value::as_str)
        {
            let pending = state::take(tool_use_id).ok_or_else(|| {
                ChannelError::Build(format!(
                    "claudeweb tool_use_id is unknown or expired: {tool_use_id}"
                ))
            })?;
            return Ok(PreparedRequest::custom_stream(Box::new(move |client| {
                Box::pin(async move { resume_turn(client, pending, tool_results).await })
            })));
        }
        let web = request::build(&parsed, ctx.upstream_model_id, prompt, timezone)?;
        let secret = ctx.secret.clone();

        Ok(PreparedRequest::custom_stream(Box::new(move |client| {
            Box::pin(async move { run_turn(client, secret, base, web).await })
        })))
    }

    fn credential_models(&self, secret: &Value) -> Option<Bytes> {
        auth::models(secret)
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, secret).await
    }

    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    fn prepare_usage_request(
        &self,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<Request<Bytes>>, ChannelError> {
        usage::request(secret, settings)
    }

    fn parse_usage(
        &self,
        status: StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        usage::parse(status, body)
    }
}

#[async_trait::async_trait]
impl ChannelLogin for ClaudeWebChannel {
    async fn cookie_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        cookie: &str,
    ) -> Result<Value, ChannelError> {
        auth::exchange(client, cookie).await
    }
}

async fn run_turn(
    client: Arc<dyn UpstreamClient>,
    secret: Value,
    base: String,
    mut web: request::WebRequest,
) -> Result<(StatusCode, http::HeaderMap, RespStream), ClientError> {
    let session_key = auth::session_key(&secret)
        .map(str::to_owned)
        .map_err(channel_error)?;
    let organization = auth::organization_uuid(&secret)
        .map(str::to_owned)
        .map_err(channel_error)?;
    let device_id = auth::device_id(&secret).map(str::to_owned);
    let conversation = crate::util::rand::uuid_v7();

    let mut file_ids = Vec::new();
    for upload in web.uploads {
        let response = upload_file(
            &client,
            &base,
            &organization,
            &session_key,
            device_id.as_deref(),
            upload,
        )
        .await?;
        if !response.status().is_success() {
            return Ok(buffered_stream(response));
        }
        let value: Value = serde_json::from_slice(response.body())
            .map_err(|e| ClientError::Transport(format!("claudeweb upload response JSON: {e}")))?;
        let file_uuid = value
            .get("file_uuid")
            .and_then(Value::as_str)
            .ok_or_else(|| ClientError::Transport("claudeweb upload missing file_uuid".into()))?;
        file_ids.push(Value::String(file_uuid.to_owned()));
    }
    web.body["files"] = Value::Array(file_ids);

    let create_url = format!("{base}/api/organizations/{organization}/chat_conversations");
    let create_body = json!({
        "uuid": conversation,
        "name": "",
        "is_temporary": true,
    });
    let mut create = json_request(http::Method::POST, &create_url, &create_body)?;
    auth::apply_browser_headers(
        &mut create,
        &session_key,
        &base,
        &format!("{base}/new?incognito"),
    )
    .map_err(channel_error)?;
    auth::apply_device_header(&mut create, device_id.as_deref()).map_err(channel_error)?;
    let response = client.send(create).await?;
    if !response.status().is_success() {
        return Ok(buffered_stream(response));
    }

    let settings_url =
        format!("{base}/api/organizations/{organization}/chat_conversations/{conversation}");
    let use_extended_thinking = web.extended_thinking && auth::is_pro(&secret);
    let settings = json!({
        "settings": {
            "paprika_mode": if use_extended_thinking { Value::String("extended".into()) } else { Value::Null }
        }
    });
    let mut update = json_request(http::Method::PUT, &settings_url, &settings)?;
    auth::apply_browser_headers(
        &mut update,
        &session_key,
        &base,
        &format!("{base}/chat/{conversation}"),
    )
    .map_err(channel_error)?;
    auth::apply_device_header(&mut update, device_id.as_deref()).map_err(channel_error)?;
    let response = client.send(update).await?;
    if !response.status().is_success() {
        return Ok(buffered_stream(response));
    }

    let completion_url = format!(
        "{base}/api/organizations/{organization}/chat_conversations/{conversation}/completion"
    );
    let mut completion = json_request(http::Method::POST, &completion_url, &web.body)?;
    completion.headers_mut().insert(
        header::ACCEPT,
        http::HeaderValue::from_static("text/event-stream"),
    );
    auth::apply_browser_headers(
        &mut completion,
        &session_key,
        &base,
        &format!("{base}/chat/{conversation}"),
    )
    .map_err(channel_error)?;
    auth::apply_device_header(&mut completion, device_id.as_deref()).map_err(channel_error)?;
    let (status, headers, stream) = client.send_streaming(completion).await?;
    if !status.is_success() {
        return Ok((status, headers, stream));
    }
    let output_tokens = Arc::new(AtomicU64::new(0));
    let decoder = sse::ClaudeWebStreamDecoder::new(web.input_tokens, Arc::clone(&output_tokens));
    let stream = state::pause_on_tool_use(
        stream,
        state::StreamMeta {
            client: Arc::clone(&client),
            base,
            organization,
            conversation,
            session_key,
            device_id,
            model: web.model,
            message_id: format!("msg_{}", crate::util::rand::uuid_v4().replace('-', "")),
            input_tokens: web.input_tokens,
            output_tokens,
        },
    );
    let stream = crate::pipeline::stream::channel_decode_stream(stream, Box::new(decoder));
    Ok((status, headers, stream))
}

async fn resume_turn(
    client: Arc<dyn UpstreamClient>,
    pending: state::Pending,
    tool_results: Vec<Value>,
) -> Result<(StatusCode, http::HeaderMap, RespStream), ClientError> {
    let tool_url = format!(
        "{}/api/organizations/{}/chat_conversations/{}/tool_result",
        pending.base, pending.organization, pending.conversation
    );
    for result in &tool_results {
        let mut request = json_request(http::Method::POST, &tool_url, result)?;
        auth::apply_browser_headers(
            &mut request,
            &pending.session_key,
            &pending.base,
            &format!("{}/chat/{}", pending.base, pending.conversation),
        )
        .map_err(channel_error)?;
        auth::apply_device_header(&mut request, pending.device_id.as_deref())
            .map_err(channel_error)?;
        let response = client.send(request).await?;
        if !response.status().is_success() {
            state::discard(pending);
            return Ok(buffered_stream(response));
        }
    }

    let prior_output = pending.output_tokens.load(Ordering::Relaxed);
    let result_tokens = tool_results
        .iter()
        .map(request::estimate_value_tokens)
        .sum::<u64>();
    let input_tokens = pending
        .input_tokens
        .saturating_add(prior_output)
        .saturating_add(result_tokens);
    let message_start = Bytes::from(
        format!(
            "event: message_start\ndata: {}\n\n",
            json!({
                "type":"message_start",
                "message":{
                    "id":pending.message_id,
                    "type":"message",
                    "role":"assistant",
                    "content":[],
                    "model":pending.model,
                    "stop_reason":null,
                    "stop_sequence":null
                }
            })
        )
        .into_bytes(),
    );
    let stream = futures_util::stream::once(async move { Ok(message_start) })
        .chain(pending.stream)
        .boxed();
    let output_tokens = Arc::new(AtomicU64::new(0));
    let decoder = sse::ClaudeWebStreamDecoder::new(input_tokens, Arc::clone(&output_tokens));
    let stream = state::pause_on_tool_use(
        stream,
        state::StreamMeta {
            client: Arc::clone(&client),
            base: pending.base,
            organization: pending.organization,
            conversation: pending.conversation,
            session_key: pending.session_key,
            device_id: pending.device_id,
            model: pending.model,
            message_id: pending.message_id,
            input_tokens,
            output_tokens,
        },
    );
    let stream = crate::pipeline::stream::channel_decode_stream(stream, Box::new(decoder));
    let mut headers = http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/event-stream"),
    );
    Ok((StatusCode::OK, headers, stream))
}

async fn upload_file(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    organization: &str,
    session_key: &str,
    device_id: Option<&str>,
    upload: request::Upload,
) -> Result<Response<Bytes>, ClientError> {
    let boundary = format!(
        "----gproxy{}",
        crate::util::rand::uuid_v4().replace('-', "")
    );
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            upload.file_name
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", upload.media_type).as_bytes());
    body.extend_from_slice(&upload.bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let url = format!("{base}/api/{organization}/upload");
    let mut req = Request::post(url)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(header::ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|e| ClientError::Transport(format!("claudeweb upload request: {e}")))?;
    auth::apply_browser_headers(&mut req, session_key, base, &format!("{base}/new"))
        .map_err(channel_error)?;
    auth::apply_device_header(&mut req, device_id).map_err(channel_error)?;
    client.send(req).await
}

fn json_request(
    method: http::Method,
    url: &str,
    value: &Value,
) -> Result<Request<Bytes>, ClientError> {
    let body = serde_json::to_vec(value)
        .map_err(|e| ClientError::Transport(format!("claudeweb request JSON: {e}")))?;
    Request::builder()
        .method(method)
        .uri(url)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(body))
        .map_err(|e| ClientError::Transport(format!("claudeweb request build: {e}")))
}

fn buffered_stream(response: Response<Bytes>) -> (StatusCode, http::HeaderMap, RespStream) {
    let (parts, body) = response.into_parts();
    let stream = futures_util::stream::once(async move { Ok::<Bytes, ClientError>(body) }).boxed();
    (parts.status, parts.headers, stream)
}

fn channel_error(error: ChannelError) -> ClientError {
    ClientError::Transport(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::protocol::{ContentGenerationKind, Operation, OperationKind};
    use crate::transform::routing::RoutingDecision;
    use async_trait::async_trait;

    #[test]
    fn non_stream_messages_force_web_stream() {
        let decision = ClaudeWebChannel
            .routing_table()
            .into_iter()
            .find(|(source, _)| {
                source.operation == Operation::GenerateContent
                    && source.kind
                        == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
            })
            .map(|(_, decision)| decision)
            .unwrap();
        let RoutingDecision::TransformTo(target) = decision else {
            panic!("claudeweb non-stream route should aggregate a web stream")
        };
        assert_eq!(target.operation, Operation::StreamGenerateContent);
        assert_eq!(
            target.kind,
            OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        );
    }

    struct ToolFlowClient {
        requests: Mutex<Vec<(String, Bytes)>>,
    }

    #[async_trait]
    impl UpstreamClient for ToolFlowClient {
        async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
            let path = request.uri().path().to_owned();
            let body = request.body().clone();
            self.requests.lock().unwrap().push((path.clone(), body));
            let status = if request.method() == http::Method::POST
                && path.ends_with("/chat_conversations")
            {
                StatusCode::CREATED
            } else if request.method() == http::Method::DELETE {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::OK
            };
            Response::builder()
                .status(status)
                .body(Bytes::new())
                .map_err(|error| ClientError::Transport(error.to_string()))
        }

        async fn send_streaming(
            &self,
            request: Request<Bytes>,
        ) -> Result<(StatusCode, http::HeaderMap, RespStream), ClientError> {
            self.requests
                .lock()
                .unwrap()
                .push((request.uri().path().to_owned(), request.body().clone()));
            let first = Bytes::from_static(
                b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_mock\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-5\"}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_mock\",\"name\":\"weather\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\\\"Singapore\\\"}\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            );
            let second = Bytes::from_static(
                b"data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_result\",\"tool_use_id\":\"toolu_mock\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\"Sunny.\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
            );
            let stream = futures_util::stream::iter([Ok(first), Ok(second)]).boxed();
            Ok((StatusCode::OK, http::HeaderMap::new(), stream))
        }
    }

    async fn collect(mut stream: RespStream) -> String {
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.unwrap());
        }
        String::from_utf8(bytes).unwrap()
    }

    #[tokio::test]
    async fn tool_result_resumes_parked_stream_and_synthesizes_usage() {
        let client = Arc::new(ToolFlowClient {
            requests: Mutex::new(Vec::new()),
        });
        let client_dyn: Arc<dyn UpstreamClient> = client.clone();
        let channel = ClaudeWebChannel;
        let settings = json!({});
        let secret = json!({
            "cookie":"sk-ant-sid-example",
            "account_uuid":"org-1",
            "capabilities":["chat"],
            "device_id":"00000000-0000-4000-8000-000000000001"
        });
        let headers = http::HeaderMap::new();
        let first_body = Bytes::from(
            json!({
                "model":"claude-sonnet-5",
                "max_tokens":128,
                "messages":[{"role":"user","content":"Use the weather tool"}],
                "tools":[{"name":"weather","description":"weather","input_schema":{"type":"object"}}]
            })
            .to_string(),
        );
        let first = channel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "claude-sonnet-5",
                method: http::Method::POST,
                path: "/v1/messages",
                query: None,
                headers: &headers,
                body: first_body,
            })
            .unwrap();
        let PreparedRequest::CustomStream(send) = first else {
            panic!("claudeweb should use a custom stream")
        };
        let (_, _, first_stream) = send(Arc::clone(&client_dyn)).await.unwrap();
        let first_text = collect(first_stream).await;
        assert!(first_text.contains("\"id\":\"toolu_mock\""));
        assert!(first_text.contains("\"stop_reason\":\"tool_use\""));
        assert!(first_text.contains("\"input_tokens\":"));

        let second_body = Bytes::from(
            json!({
                "model":"claude-sonnet-5",
                "max_tokens":128,
                "messages":[
                    {"role":"assistant","content":[{"type":"tool_use","id":"toolu_mock","name":"weather","input":{"city":"Singapore"}}]},
                    {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_mock","content":"Sunny, 31 C"}]}
                ]
            })
            .to_string(),
        );
        let second = channel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "claude-sonnet-5",
                method: http::Method::POST,
                path: "/v1/messages",
                query: None,
                headers: &headers,
                body: second_body,
            })
            .unwrap();
        let PreparedRequest::CustomStream(send) = second else {
            panic!("tool result should resume a custom stream")
        };
        let (_, _, second_stream) = send(client_dyn).await.unwrap();
        let second_text = collect(second_stream).await;
        assert!(second_text.contains("event: message_start"));
        assert!(second_text.contains("Sunny."));
        assert!(!second_text.contains("\"type\":\"tool_result\""));
        assert!(second_text.contains("\"output_tokens\":"));

        let requests = client.requests.lock().unwrap();
        let result_request = requests
            .iter()
            .find(|(path, _)| path.ends_with("/tool_result"))
            .expect("tool_result request");
        let sent: Value = serde_json::from_slice(&result_request.1).unwrap();
        assert_eq!(sent["tool_use_id"], "toolu_mock");
        assert_eq!(sent["content"][0]["text"], "Sunny, 31 C");
    }

    #[tokio::test]
    #[ignore = "requires CLAUDE_SESSION_KEY and live claude.ai access"]
    async fn live_tool_result_round_trip() {
        let cookie = std::env::var("CLAUDE_SESSION_KEY").expect("CLAUDE_SESSION_KEY");
        let organization = std::env::var("CLAUDE_ORGANIZATION_UUID").ok();
        let device_id = std::env::var("CLAUDE_DEVICE_ID").ok();
        let channel = ClaudeWebChannel;
        let client: Arc<dyn UpstreamClient> = Arc::new(
            crate::http::client::WreqClient::with_proxy_and_emulation(
                None,
                channel.default_emulation(),
            )
            .expect("browser client"),
        );
        let secret = match organization {
            Some(organization) => json!({
                "cookie":cookie,
                "account_uuid":organization,
                "capabilities":["chat"],
                "device_id":device_id.unwrap_or_else(crate::util::rand::uuid_v4)
            }),
            None => channel.cookie_exchange(&client, &cookie).await.unwrap(),
        };
        let settings = json!({"timezone":"Asia/Singapore"});
        let headers = http::HeaderMap::new();
        let first_body = Bytes::from(
            json!({
                "model":"claude-sonnet-5",
                "max_tokens":256,
                "messages":[{"role":"user","content":"Call get_weather exactly once for Singapore, then wait for its result."}],
                "tools":[{"name":"get_weather","description":"Get current weather","input_schema":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}}]
            })
            .to_string(),
        );
        let first = channel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "claude-sonnet-5",
                method: http::Method::POST,
                path: "/v1/messages",
                query: None,
                headers: &headers,
                body: first_body,
            })
            .unwrap();
        let PreparedRequest::CustomStream(send) = first else {
            panic!("custom stream")
        };
        let (status, _, stream) = send(Arc::clone(&client)).await.unwrap();
        let first_text = collect(stream).await;
        assert_eq!(status, StatusCode::OK, "{first_text}");
        let tool_use_id = first_text
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find_map(|event| {
                event
                    .get("content_block")
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
                    .and_then(|block| block.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .expect("live tool_use");
        assert!(first_text.contains("\"stop_reason\":\"tool_use\""));

        let second_body = Bytes::from(
            json!({
                "model":"claude-sonnet-5",
                "max_tokens":256,
                "messages":[
                    {"role":"assistant","content":[{"type":"tool_use","id":tool_use_id,"name":"get_weather","input":{"city":"Singapore"}}]},
                    {"role":"user","content":[{"type":"tool_result","tool_use_id":tool_use_id,"content":"Sunny, 31 C"}]}
                ]
            })
            .to_string(),
        );
        let second = channel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "claude-sonnet-5",
                method: http::Method::POST,
                path: "/v1/messages",
                query: None,
                headers: &headers,
                body: second_body,
            })
            .unwrap();
        let PreparedRequest::CustomStream(send) = second else {
            panic!("resumed custom stream")
        };
        let (status, _, stream) = send(client).await.unwrap();
        let second_text = collect(stream).await;
        assert_eq!(status, StatusCode::OK, "{second_text}");
        assert!(
            second_text.contains("\"type\":\"text_delta\""),
            "{second_text}"
        );
        assert!(second_text.contains("\"stop_reason\":\"end_turn\""));
        assert!(second_text.contains("event: message_stop"));
        assert!(second_text.contains("\"output_tokens\":"));
        assert!(!second_text.contains("\"type\":\"tool_result\""));
    }
}
