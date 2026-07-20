//! Multi-step claude.ai conversation session orchestration.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, Response, StatusCode, header};
use serde_json::{Value, json};

use super::{auth, request, response, state};
use crate::channel::{ChannelError, PrepareCtx, PreparedRequest};
use crate::http::client::{ClientError, RespStream, UpstreamClient};

pub(super) fn prepare(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("UTC");
    let parsed: Value = serde_json::from_slice(&ctx.body)
        .map_err(|error| ChannelError::Build(format!("claudeweb request JSON: {error}")))?;
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
        let value: Value = serde_json::from_slice(response.body()).map_err(|error| {
            ClientError::Transport(format!("claudeweb upload response JSON: {error}"))
        })?;
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
    let decoder =
        response::ClaudeWebStreamDecoder::new(web.input_tokens, Arc::clone(&output_tokens));
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
    let decoder = response::ClaudeWebStreamDecoder::new(input_tokens, Arc::clone(&output_tokens));
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
        .map_err(|error| ClientError::Transport(format!("claudeweb upload request: {error}")))?;
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
        .map_err(|error| ClientError::Transport(format!("claudeweb request JSON: {error}")))?;
    Request::builder()
        .method(method)
        .uri(url)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(body))
        .map_err(|error| ClientError::Transport(format!("claudeweb request build: {error}")))
}

fn buffered_stream(response: Response<Bytes>) -> (StatusCode, http::HeaderMap, RespStream) {
    let (parts, body) = response.into_parts();
    let stream = futures_util::stream::once(async move { Ok::<Bytes, ClientError>(body) }).boxed();
    (parts.status, parts.headers, stream)
}

fn channel_error(error: ChannelError) -> ClientError {
    ClientError::Transport(error.to_string())
}
