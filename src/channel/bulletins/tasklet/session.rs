use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, Response, StatusCode, header};
use serde_json::{Value, json};

use super::{auth, bridge, request, response, stream};
use crate::channel::{ChannelError, PrepareCtx, PreparedRequest};
use crate::http::client::{ClientError, ConduitSocket, RespStream, UpstreamClient};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub fn prepare(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(auth::DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned();
    let timezone = ctx
        .provider_settings
        .get("timezone")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("UTC")
        .to_owned();
    let emit_tool_trace = ctx
        .provider_settings
        .get("emit_tool_trace")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let token = auth::session_token(ctx.secret)?.to_owned();
    let workspace = auth::workspace_id(ctx.secret)?.to_owned();
    let task = request::parse(&ctx.body, ctx.upstream_model_id)?;
    let downstream_stream = ctx.stream;

    Ok(PreparedRequest::custom_stream(Box::new(move |client| {
        Box::pin(async move {
            run(
                client,
                base,
                token,
                workspace,
                timezone,
                task,
                emit_tool_trace,
                downstream_stream,
            )
            .await
        })
    })))
}

#[allow(clippy::too_many_arguments)]
async fn run(
    client: Arc<dyn UpstreamClient>,
    base: String,
    token: String,
    workspace: String,
    timezone: String,
    mut task: request::TaskletRequest,
    emit_tool_trace: bool,
    downstream_stream: bool,
) -> Result<(StatusCode, http::HeaderMap, RespStream), ClientError> {
    let turn = bridge::register(&task.tools);
    if let Some(turn) = &turn {
        request::attach_tool_bridge(&mut task, turn.id())
            .map_err(|error| ClientError::Config(error.to_string()))?;
    }
    let mut uploaded = Vec::new();
    for upload in &task.uploads {
        uploaded.push(upload_file(&client, &base, &token, upload).await?);
    }
    let body = request::send_body(&task, uploaded, &workspace, &timezone);
    let response = send_json(&client, &base, &token, "/api/sendChatMessage", &body).await?;
    if !response.status().is_success() {
        return Ok(buffered_stream(response));
    }
    let value: Value = serde_json::from_slice(response.body())
        .map_err(|error| ClientError::Transport(format!("tasklet send response: {error}")))?;
    let agent_id = value
        .get("agentId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ClientError::Transport("tasklet send response missing agentId".into()))?;
    let mut socket = open_socket(&client, &base, &token).await?;
    socket
        .send_text(json!({"type":"startSync","agentId":agent_id}).to_string())
        .await?;
    socket
        .send_text(json!({"type":"subscribeBlocks","runId":agent_id,"pageSize":100}).to_string())
        .await?;

    let synth = response::Synth::new(task.model, emit_tool_trace);
    let stream = stream::create(socket, synth, turn)?;
    let mut headers = http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        http::HeaderValue::from_static(if downstream_stream {
            "text/event-stream"
        } else {
            "application/json"
        }),
    );
    headers.insert(
        header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-cache"),
    );
    Ok((StatusCode::OK, headers, stream))
}

async fn open_socket(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    token: &str,
) -> Result<Box<dyn ConduitSocket>, ClientError> {
    let ws_url = websocket_url(base)?;
    let request = Request::get(ws_url)
        .header(header::ORIGIN, "https://tasklet.ai")
        .body(Bytes::new())
        .map_err(|error| ClientError::Transport(format!("tasklet websocket request: {error}")))?;
    let mut socket = client.open_websocket(request).await?;
    socket
        .send_text(json!({"type":"connect","sessionToken":token}).to_string())
        .await?;
    loop {
        let received = tokio::time::timeout(CONNECT_TIMEOUT, socket.recv_text())
            .await
            .map_err(|_| ClientError::Transport("tasklet websocket connect timeout".into()))?;
        let text = received.ok_or_else(|| {
            ClientError::Transport("tasklet websocket closed during connect".into())
        })??;
        let frame: Value = serde_json::from_str(&text)
            .map_err(|error| ClientError::Transport(format!("tasklet websocket JSON: {error}")))?;
        match frame.get("type").and_then(Value::as_str) {
            Some("connected") => return Ok(socket),
            Some("error") => {
                return Err(ClientError::Transport(format!(
                    "tasklet websocket connect: {}",
                    frame
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                )));
            }
            _ => {}
        }
    }
}

async fn send_json(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    token: &str,
    path: &str,
    value: &Value,
) -> Result<Response<Bytes>, ClientError> {
    let body = serde_json::to_vec(value)
        .map_err(|error| ClientError::Transport(format!("tasklet request JSON: {error}")))?;
    let mut request = Request::post(format!("{base}{path}"))
        .body(Bytes::from(body))
        .map_err(|error| ClientError::Transport(format!("tasklet request build: {error}")))?;
    auth::apply(&mut request, token, true)
        .map_err(|error| ClientError::Transport(error.to_string()))?;
    client.send(request).await
}

async fn upload_file(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    token: &str,
    upload: &request::Upload,
) -> Result<String, ClientError> {
    let boundary = format!(
        "----gproxy{}",
        crate::util::rand::uuid_v4().replace('-', "")
    );
    let filename = upload.file_name.replace(['\r', '\n', '"'], "_");
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", upload.media_type).as_bytes());
    body.extend_from_slice(&upload.bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut request = Request::post(format!("{base}/api/files/upload"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Bytes::from(body))
        .map_err(|error| ClientError::Transport(format!("tasklet upload request: {error}")))?;
    auth::apply(&mut request, token, false)
        .map_err(|error| ClientError::Transport(error.to_string()))?;
    let response = client.send(request).await?;
    if !response.status().is_success() {
        return Err(ClientError::Transport(format!(
            "tasklet upload failed: {}",
            response.status()
        )));
    }
    let value: Value = serde_json::from_slice(response.body())
        .map_err(|error| ClientError::Transport(format!("tasklet upload response: {error}")))?;
    value
        .get("fileId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| ClientError::Transport("tasklet upload response missing fileId".into()))
}

fn websocket_url(base: &str) -> Result<String, ClientError> {
    if let Some(rest) = base.strip_prefix("https://") {
        Ok(format!("wss://{rest}/api/sync"))
    } else if let Some(rest) = base.strip_prefix("http://") {
        Ok(format!("ws://{rest}/api/sync"))
    } else {
        Err(ClientError::Config(
            "tasklet base_url must be HTTP(S)".into(),
        ))
    }
}

fn buffered_stream(response: Response<Bytes>) -> (StatusCode, http::HeaderMap, RespStream) {
    let (parts, body) = response.into_parts();
    let stream = futures_util::stream::once(async move { Ok::<Bytes, ClientError>(body) }).boxed();
    (parts.status, parts.headers, stream)
}
