//! Public Codex PAT account/service surface over the normal user-key boundary.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{LazyLock, Mutex};

use base64::Engine as _;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::channel::{CredentialControlOperation, Disposition};
use crate::pipeline::context::{RequestCtx, RoutingMode};
use crate::pipeline::error::PipelineError;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::store::persistence::records::CodexTaskBindingInput;
use crate::store::persistence::records::{Provider, Quota, Scope};
use crate::util::time::unix_now;

pub const FILE_UPLOAD_MAX_BYTES: usize = 512 * 1024 * 1024;
const FILE_REQUEST_MAX_BYTES: usize = FILE_UPLOAD_MAX_BYTES + 1024 * 1024;
static FILE_UPLOADS_IN_FLIGHT: LazyLock<Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn scoped_file_provider_name(path: &str) -> Option<&str> {
    let segments = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    match segments.as_slice() {
        [name, "v1", "files"] => Some(*name),
        _ => None,
    }
}

pub fn request_body_limit(state: &AppState, method: &str, path: &str) -> usize {
    let upload_provider = (method == "POST")
        .then(|| scoped_file_provider_name(path))
        .flatten()
        .and_then(|name| state.cp().providers_by_name.get(name).cloned())
        .is_some_and(|provider| {
            provider.enabled && matches!(provider.channel.as_str(), "openai" | "codex")
        });
    if upload_provider {
        FILE_REQUEST_MAX_BYTES
    } else {
        crate::config::MAX_BODY_BYTES
    }
}

fn clamp_upload_limit(limit: u64) -> usize {
    usize::try_from(limit).unwrap_or(usize::MAX)
}

pub struct FileUploadPermit {
    keys: [String; 2],
}

impl Drop for FileUploadPermit {
    fn drop(&mut self) {
        let mut in_flight = FILE_UPLOADS_IN_FLIGHT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for key in &self.keys {
            let remove = if let Some(count) = in_flight.get_mut(key) {
                *count = count.saturating_sub(1);
                *count == 0
            } else {
                false
            };
            if remove {
                in_flight.remove(key);
            }
        }
    }
}

pub fn try_file_upload_permits(
    state: &AppState,
    method: &str,
    path: &str,
) -> Result<Option<FileUploadPermit>, PipelineError> {
    let Some(provider_name) = (method == "POST")
        .then(|| scoped_file_provider_name(path))
        .flatten()
    else {
        return Ok(None);
    };
    let Some(provider) = state
        .cp()
        .providers_by_name
        .get(provider_name)
        .filter(|provider| {
            provider.enabled && matches!(provider.channel.as_str(), "openai" | "codex")
        })
        .cloned()
    else {
        return Ok(None);
    };
    #[cfg(not(target_arch = "wasm32"))]
    let env_limit = std::env::var("GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(clamp_upload_limit);
    #[cfg(target_arch = "wasm32")]
    let env_limit: Option<usize> = None;
    let stored_limit =
        (env_limit.is_none()).then(|| clamp_upload_limit(state.cp().file_upload_max_in_flight));
    let provider_limit = provider
        .settings_json
        .get("file_upload_max_in_flight")
        .and_then(Value::as_u64)
        .map(clamp_upload_limit);
    let keys = ["global".to_owned(), format!("provider:{provider_name}")];
    let limits = [
        env_limit.or(stored_limit).unwrap_or(0),
        provider_limit.unwrap_or(0),
    ];
    let mut in_flight = FILE_UPLOADS_IN_FLIGHT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if keys
        .iter()
        .zip(limits)
        .any(|(key, limit)| limit > 0 && in_flight.get(key).copied().unwrap_or(0) >= limit)
    {
        return Err(PipelineError::RateLimited {
            retry_after_secs: 1,
        });
    }
    for key in &keys {
        let count = in_flight.entry(key.clone()).or_default();
        *count = count.saturating_add(1);
    }
    drop(in_flight);
    Ok(Some(FileUploadPermit { keys }))
}

pub async fn execute(
    state: &AppState,
    mut ctx: RequestCtx,
) -> Option<Result<ExecOutcome, PipelineError>> {
    if !is_service_path(&ctx.path) {
        return None;
    }
    if (ctx.path == "/v1/files" || ctx.path.starts_with("/v1/files/"))
        && let RoutingMode::Named { name } = &ctx.mode
        && state
            .cp()
            .providers_by_name
            .get(name)
            .is_some_and(|provider| provider.channel != "codex")
    {
        return None;
    }
    Some(run(state, &mut ctx).await)
}

pub fn is_remote_control_websocket_ingress(path: &str) -> bool {
    path.ends_with("/backend-api/wham/remote/control/server")
        || path.ends_with("/api/codex/remote/control/server")
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn open_remote_control_websocket(
    state: &AppState,
    mut ctx: RequestCtx,
) -> Result<Box<dyn crate::http::client::ConduitSocket>, PipelineError> {
    ctx.path = canonical_service_path(&ctx.path);
    if ctx.method != Method::GET || ctx.path != "/api/codex/remote/control/server" {
        return Err(PipelineError::UnsupportedPath);
    }
    let remote_token = crate::util::api_key::extract_bearer(&ctx.headers, ctx.query.as_deref())
        .ok_or(PipelineError::Unauthorized)?;
    let cp = state.cp();
    let provider_name = match &ctx.mode {
        RoutingMode::Named { name } | RoutingMode::Scoped { provider: name } => name,
        _ => return Err(PipelineError::UnsupportedPath),
    };
    let provider = cp
        .providers_by_name
        .get(provider_name)
        .filter(|provider| provider.enabled && provider.channel == "codex")
        .cloned()
        .ok_or_else(|| PipelineError::UnknownProvider(provider_name.clone()))?;
    let binding_key = remote_control_token_key(provider.id, &remote_token);
    drop(cp);
    let credential_id = state
        .cache
        .get(&binding_key)
        .await
        .and_then(|value| String::from_utf8(value).ok())
        .and_then(|value| value.split(':').next()?.parse::<i64>().ok())
        .ok_or(PipelineError::Unauthorized)?;
    let mut headers = forwarded_headers(&ctx.headers);
    headers.insert(
        http::header::AUTHORIZATION,
        http::HeaderValue::from_str(&format!("Bearer {remote_token}"))
            .map_err(|_| PipelineError::Unauthorized)?,
    );
    let operation = CredentialControlOperation::CodexRaw {
        label: "remote_control_ws",
        method: Method::GET,
        path: "/wham/remote/control/server".to_owned(),
        query: ctx.query,
        headers,
        body: Bytes::new(),
    };
    crate::credentials::control::open_raw_websocket(state, credential_id, operation)
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))
}

fn is_service_path(path: &str) -> bool {
    path == "/v1/user-auth-credential/whoami"
        || path == "/v1/memories/trace_summarize"
        || path == "/v1/files"
        || path.starts_with("/v1/files/")
        || path == "/api/codex"
        || path.starts_with("/api/codex/")
        || path == "/backend-api"
        || path.starts_with("/backend-api/")
        || path.starts_with("/codex/")
        || path.starts_with("/ps/")
}

async fn run(state: &AppState, ctx: &mut RequestCtx) -> Result<ExecOutcome, PipelineError> {
    ctx.path = canonical_service_path(&ctx.path);
    let cp = state.cp();
    let identity = crate::pipeline::auth::authenticate(&cp, &ctx.headers, ctx.query.as_deref())?;
    let provider_name = match &ctx.mode {
        RoutingMode::Named { name } | RoutingMode::Scoped { provider: name } => name,
        _ => return Err(PipelineError::UnsupportedPath),
    };
    let provider = cp
        .providers_by_name
        .get(provider_name)
        .filter(|provider| provider.enabled && provider.channel == "codex")
        .cloned()
        .ok_or_else(|| PipelineError::UnknownProvider(provider_name.clone()))?;
    let service = service_name(&ctx.path);
    let authorization =
        crate::pipeline::authz::prepare_provider_service(&cp, &identity, &provider.name, service)?;
    drop(cp);
    crate::pipeline::authz::authorize(&authorization, state.cache.as_ref(), unix_now()).await?;
    ctx.identity = Some(identity.clone());

    match (ctx.method.as_str(), ctx.path.as_str()) {
        ("GET", "/v1/user-auth-credential/whoami") => {
            return Ok(json_outcome(StatusCode::OK, whoami(&identity, &provider)));
        }
        ("GET", "/api/codex/usage") => {
            return Ok(json_outcome(
                StatusCode::OK,
                virtual_usage(state, &identity, &provider).await,
            ));
        }
        ("POST", "/api/codex/usage/thread_usage/query") => {
            return virtual_thread_usage(state, &identity, &provider, &ctx.body).await;
        }
        ("GET", "/api/codex/accounts/check") => {
            return Ok(json_outcome(
                StatusCode::OK,
                virtual_account(&identity, &provider),
            ));
        }
        ("GET", "/api/codex/profiles/me") => {
            return virtual_profile(state, &identity, &provider).await;
        }
        ("GET", "/api/codex/settings/user") => {
            return Ok(json_outcome(
                StatusCode::OK,
                provider
                    .settings_json
                    .get("codex_virtual_settings")
                    .cloned()
                    .unwrap_or_else(|| json!({ "commit_attribution_enabled": false })),
            ));
        }
        ("GET", "/api/codex/workspace-messages") => {
            return Ok(json_outcome(
                StatusCode::OK,
                json!({
                    "messages": provider.settings_json
                        .get("codex_workspace_messages")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                }),
            ));
        }
        ("GET", "/api/codex/config/bundle") => {
            return Ok(json_outcome(
                StatusCode::OK,
                provider
                    .settings_json
                    .get("codex_config_bundle")
                    .cloned()
                    .unwrap_or_else(|| json!({ "config_toml": null, "requirements_toml": null })),
            ));
        }
        ("GET", "/api/codex/rate-limit-reset-credits") => {
            return Ok(json_outcome(
                StatusCode::OK,
                json!({ "available_count": 0, "credits": [] }),
            ));
        }
        ("POST", "/api/codex/rate-limit-reset-credits/consume") => {
            return Ok(json_outcome(
                StatusCode::OK,
                json!({ "code": "no_credit", "windows_reset": 0 }),
            ));
        }
        ("POST", "/api/codex/accounts/send_add_credits_nudge_email") => {
            return Ok(json_outcome(StatusCode::OK, json!({})));
        }
        ("POST", "/api/codex/analytics-events/events")
        | ("POST", "/backend-api/codex/analytics-events/events") => {
            tracing::info!(
                user_id = identity.user.id,
                "accepted virtual Codex analytics events"
            );
            return Ok(json_outcome(StatusCode::OK, json!({})));
        }
        _ => {}
    }

    if ctx.method == Method::GET && ctx.path == "/api/codex/tasks/list" {
        return local_task_list(state, &identity, &provider, ctx.query.as_deref()).await;
    }
    if ctx.path == "/api/codex/files"
        || ctx.path.starts_with("/api/codex/files/")
        || ctx.path == "/v1/files"
        || ctx.path.starts_with("/v1/files/")
    {
        return file_request(state, &identity, &provider, ctx).await;
    }
    if ctx.method == Method::GET
        && (ctx.path == "/api/codex/environments"
            || ctx.path.starts_with("/api/codex/environments/by-repo/"))
    {
        return aggregate_environments(state, &provider, ctx).await;
    }
    if ctx.method == Method::POST && ctx.path == "/api/codex/tasks" {
        return create_task(state, &identity, &provider, ctx).await;
    }
    if let Some(task_id) = task_id_from_path(&ctx.path) {
        return bound_task_request(state, &identity, &provider, ctx, &task_id).await;
    }

    let (label, upstream_path) =
        allowlisted_upstream(&ctx.method, &ctx.path).ok_or(PipelineError::UnsupportedPath)?;
    let operation = CredentialControlOperation::CodexRaw {
        label,
        method: ctx.method.clone(),
        path: upstream_path,
        query: ctx.query.clone(),
        headers: forwarded_headers(&ctx.headers),
        body: ctx.body.clone(),
    };
    execute_balanced(
        state,
        &provider,
        operation,
        is_retryable(&ctx.method, &ctx.path),
        identity.user.id,
    )
    .await
}

fn canonical_service_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("/backend-api/wham/") {
        format!("/api/codex/{rest}")
    } else if let Some(rest) = path.strip_prefix("/backend-api/codex/") {
        format!("/api/codex/{rest}")
    } else if let Some(rest) = path.strip_prefix("/backend-api/") {
        format!("/api/codex/{rest}")
    } else if let Some(rest) = path.strip_prefix("/codex/") {
        format!("/api/codex/{rest}")
    } else if let Some(rest) = path.strip_prefix("/ps/") {
        format!("/api/codex/ps/{rest}")
    } else {
        path.to_owned()
    }
}

async fn virtual_thread_usage(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Provider,
    body: &[u8],
) -> Result<ExecOutcome, PipelineError> {
    let ids = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("thread_ids").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let mut threads = Vec::new();
    for thread_id in ids.iter().filter_map(Value::as_str) {
        let rows = state
            .persistence
            .query_usages(&crate::store::persistence::UsageQuery {
                provider_id: Some(provider.id),
                user_id: Some(identity.user.id),
                thread_id: Some(thread_id.to_owned()),
                limit: 10_000,
                ..Default::default()
            })
            .await
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
        if rows.is_empty() {
            continue;
        }
        let input_tokens: i64 = rows.iter().map(|row| row.input_tokens).sum();
        let output_tokens: i64 = rows.iter().map(|row| row.output_tokens).sum();
        let cached_input_tokens: i64 = rows.iter().map(|row| row.cache_read_tokens).sum();
        let cost = rows
            .iter()
            .fold(rust_decimal::Decimal::ZERO, |sum, row| sum + row.cost);
        let micros = (cost.to_string().parse::<f64>().unwrap_or(0.0) * 1_000_000.0).round() as i64;
        threads.push(json!({
            "thread_id": thread_id,
            "estimated_usage_credits_micros": micros,
            "estimated_usage_usd_micros": micros,
            "groups": [{
                "model": null,
                "reasoning_effort": null,
                "speed": null,
                "estimated_usage_credits_micros": micros,
                "net_new_input_tokens": input_tokens.saturating_sub(cached_input_tokens),
                "cached_input_tokens": cached_input_tokens,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "total_tokens": input_tokens.saturating_add(output_tokens)
            }]
        }));
    }
    Ok(json_outcome(StatusCode::OK, json!({ "threads": threads })))
}

async fn file_request(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    if ctx.method == Method::GET && ctx.path == "/v1/files" {
        let params = ctx
            .query
            .as_deref()
            .and_then(|query| serde_urlencoded::from_str::<Vec<(String, String)>>(query).ok())
            .unwrap_or_default();
        let param = |name: &str| {
            params
                .iter()
                .find_map(|(key, value)| (key == name).then_some(value.as_str()))
        };
        let purpose = param("purpose");
        let after = param("after");
        let limit = param("limit")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(10_000)
            .clamp(1, 10_000);
        let mut rows = state
            .persistence
            .list_codex_task_bindings(provider.id, identity.user.id)
            .await
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
        rows.sort_by_key(|row| std::cmp::Reverse((row.updated_at, row.id)));
        let data = rows
            .into_iter()
            .filter(|row| {
                row.task_id.starts_with("file:")
                    && row.summary_json.get("deleted").and_then(Value::as_bool) != Some(true)
            })
            .filter_map(|row| row.summary_json.get("file").cloned())
            .skip_while(|file| {
                after.is_some_and(|after| file.get("id").and_then(Value::as_str) != Some(after))
            })
            .skip(if after.is_some() { 1 } else { 0 })
            .filter(|file| {
                purpose.is_none_or(|purpose| {
                    file.get("purpose").and_then(Value::as_str) == Some(purpose)
                })
            })
            .take(limit)
            .collect::<Vec<_>>();
        return Ok(json_outcome(
            StatusCode::OK,
            json!({ "object": "list", "data": data, "has_more": false }),
        ));
    }
    if ctx.method == Method::POST && ctx.path == "/api/codex/files" {
        return hosted_file_create(state, identity, provider, ctx.body.clone(), None).await;
    }
    if ctx.method == Method::POST && ctx.path == "/v1/files" {
        let upload = crate::pipeline::ingress::parse_file_multipart(&ctx.body, &ctx.headers)?;
        if upload.file.len() > FILE_UPLOAD_MAX_BYTES {
            return Err(PipelineError::PayloadTooLarge);
        }
        let metadata = json!({ "file_name": upload.filename, "file_size": upload.file.len(), "use_case": "codex" });
        let created = hosted_file_create_raw(
            state,
            identity,
            provider,
            Bytes::from(serde_json::to_vec(&metadata).unwrap_or_default()),
        )
        .await?;
        let upload_url = created
            .1
            .get("upload_url")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PipelineError::Transport("hosted file response missing upload_url".into())
            })?;
        let credential = created.0;
        let file_id = created
            .1
            .get("file_id")
            .and_then(Value::as_str)
            .ok_or_else(|| PipelineError::Transport("hosted file response missing file_id".into()))?
            .to_owned();
        let pending = json!({ "id": file_id.clone(), "object": "file", "bytes": metadata["file_size"], "created_at": unix_now(), "filename": metadata["file_name"], "purpose": upload.purpose.clone(), "status": "uploaded", "status_details": null });
        save_file_binding(
            state,
            identity,
            provider,
            credential.id,
            &file_id,
            pending,
            created.1.clone(),
        )
        .await?;
        let channel = state
            .channels
            .get("codex")
            .ok_or_else(|| PipelineError::UnknownChannel("codex".into()))?;
        let client = state
            .upstream_client_for_credential(&channel, &credential, provider)
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
        let request = http::Request::put(upload_url)
            .header("x-ms-blob-type", "BlockBlob")
            .header(http::header::CONTENT_TYPE, upload.mime_type.as_str())
            .body(upload.file)
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
        let uploaded = client
            .send(request)
            .await
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
        if !uploaded.status().is_success() {
            return Err(PipelineError::Transport("hosted blob upload failed".into()));
        }
        let finalized = finalize_hosted_file(state, provider, credential.id, &file_id).await?;
        let now = unix_now();
        let object = json!({ "id": file_id.clone(), "object": "file", "bytes": metadata["file_size"], "created_at": now, "filename": metadata["file_name"], "purpose": upload.purpose, "status": "processed", "status_details": null });
        save_file_binding(
            state,
            identity,
            provider,
            credential.id,
            &file_id,
            object.clone(),
            finalized,
        )
        .await?;
        return Ok(json_outcome(StatusCode::OK, object));
    }
    enum FileAction {
        Retrieve,
        Delete,
        Content,
        Finalize,
    }

    let (file_id, action) = if let Some(rest) = ctx.path.strip_prefix("/v1/files/") {
        if let Some(id) = rest
            .strip_suffix("/content")
            .filter(|id| !id.is_empty() && !id.contains('/'))
        {
            if ctx.method != Method::GET {
                return Err(PipelineError::UnsupportedPath);
            }
            (id, FileAction::Content)
        } else if !rest.is_empty() && !rest.contains('/') {
            match ctx.method.as_str() {
                "GET" => (rest, FileAction::Retrieve),
                "DELETE" => (rest, FileAction::Delete),
                _ => return Err(PipelineError::UnsupportedPath),
            }
        } else {
            return Err(PipelineError::UnsupportedPath);
        }
    } else if let Some(id) = ctx
        .path
        .strip_prefix("/api/codex/files/")
        .and_then(|rest| rest.strip_suffix("/uploaded"))
        .filter(|id| !id.is_empty() && !id.contains('/'))
    {
        if ctx.method != Method::POST {
            return Err(PipelineError::UnsupportedPath);
        }
        (id, FileAction::Finalize)
    } else {
        return Err(PipelineError::UnsupportedPath);
    };
    let binding = state
        .persistence
        .get_codex_task_binding(provider.id, &format!("file:{file_id}"))
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?
        .filter(|row| row.owner_user_id == identity.user.id)
        .ok_or(PipelineError::UnsupportedPath)?;
    if matches!(action, FileAction::Delete) {
        let mut summary = binding.summary_json.clone();
        summary["deleted"] = Value::Bool(true);
        state
            .persistence
            .upsert_codex_task_binding(CodexTaskBindingInput {
                provider_id: provider.id,
                task_id: binding.task_id,
                credential_id: binding.credential_id,
                owner_user_id: binding.owner_user_id,
                environment_id: None,
                summary_json: summary,
            })
            .await
            .map_err(|e| PipelineError::Transport(e.to_string()))?;
        return Ok(json_outcome(
            StatusCode::OK,
            json!({ "id": file_id, "object": "file", "deleted": true }),
        ));
    }
    if binding.summary_json.get("deleted").and_then(Value::as_bool) == Some(true) {
        return Err(PipelineError::UnsupportedPath);
    }
    if matches!(action, FileAction::Content) {
        let finalized =
            finalize_hosted_file(state, provider, binding.credential_id, file_id).await?;
        let url = finalized
            .get("download_url")
            .and_then(Value::as_str)
            .ok_or_else(|| PipelineError::Transport("file download URL missing".into()))?;
        let channel = state
            .channels
            .get("codex")
            .ok_or_else(|| PipelineError::UnknownChannel("codex".into()))?;
        let credential = state
            .persistence
            .get_credential(binding.credential_id)
            .await
            .map_err(|e| PipelineError::Transport(e.to_string()))?
            .ok_or(PipelineError::NoCredentials)?;
        let client = state
            .upstream_client_for_credential(&channel, &credential, provider)
            .map_err(|e| PipelineError::Transport(e.to_string()))?;
        let request = http::Request::get(url)
            .body(Bytes::new())
            .map_err(|e| PipelineError::Transport(e.to_string()))?;
        let response = client
            .send(request)
            .await
            .map_err(|e| PipelineError::Transport(e.to_string()))?;
        let (parts, body) = response.into_parts();
        return Ok(ExecOutcome {
            status: parts.status,
            headers: parts.headers,
            body: ResponseBody::Full(body),
            disposition: Disposition::Success,
        });
    }
    if matches!(action, FileAction::Finalize) {
        let value = finalize_hosted_file(state, provider, binding.credential_id, file_id).await?;
        let mut summary = binding.summary_json.clone();
        summary["hosted"] = value.clone();
        summary["file"]["status"] = Value::String("processed".to_owned());
        state
            .persistence
            .upsert_codex_task_binding(CodexTaskBindingInput {
                provider_id: provider.id,
                task_id: binding.task_id,
                credential_id: binding.credential_id,
                owner_user_id: binding.owner_user_id,
                environment_id: None,
                summary_json: summary,
            })
            .await
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
        return Ok(json_outcome(StatusCode::OK, value));
    }
    Ok(json_outcome(
        StatusCode::OK,
        binding
            .summary_json
            .get("file")
            .cloned()
            .unwrap_or(Value::Null),
    ))
}

async fn hosted_file_create_raw(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    body: Bytes,
) -> Result<(Arc<crate::store::persistence::records::Credential>, Value), PipelineError> {
    let credentials = {
        let cp = state.cp();
        crate::pipeline::balance::service_credentials(&cp, provider, state.health.as_ref(), None)
    };
    let credential = credentials
        .first()
        .cloned()
        .ok_or(PipelineError::NoCredentials)?;
    let operation = CredentialControlOperation::CodexRaw {
        label: "hosted_file_create",
        method: Method::POST,
        path: "/files".into(),
        query: None,
        headers: HeaderMap::new(),
        body,
    };
    let response = crate::credentials::control::execute_raw(state, credential.id, operation)
        .await
        .map_err(|e| PipelineError::Transport(e.to_string()))?;
    if !response.status.is_success() {
        return Err(PipelineError::Transport("hosted file create failed".into()));
    }
    let value = serde_json::from_slice(&response.body)
        .map_err(|e| PipelineError::Transport(e.to_string()))?;
    let _ = identity;
    Ok((credential, value))
}

async fn hosted_file_create(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    body: Bytes,
    purpose: Option<String>,
) -> Result<ExecOutcome, PipelineError> {
    let metadata = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let file_size = metadata
        .get("file_size")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if file_size > FILE_UPLOAD_MAX_BYTES as u64 {
        return Err(PipelineError::PayloadTooLarge);
    }
    let (credential, value) = hosted_file_create_raw(state, identity, provider, body).await?;
    let file_id = value
        .get("file_id")
        .and_then(Value::as_str)
        .ok_or_else(|| PipelineError::Transport("hosted file response missing file_id".into()))?;
    let filename = metadata
        .get("file_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let purpose = purpose
        .or_else(|| {
            metadata
                .get("use_case")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "codex".to_owned());
    let object = json!({ "id": file_id, "object": "file", "bytes": file_size, "created_at": unix_now(), "filename": filename, "purpose": purpose, "status": "uploaded", "status_details": null });
    save_file_binding(
        state,
        identity,
        provider,
        credential.id,
        file_id,
        object,
        value.clone(),
    )
    .await?;
    Ok(json_outcome(StatusCode::OK, value))
}

async fn finalize_hosted_file(
    state: &AppState,
    _provider: &Arc<Provider>,
    credential_id: i64,
    file_id: &str,
) -> Result<Value, PipelineError> {
    for attempt in 0..120 {
        let operation = CredentialControlOperation::CodexRaw {
            label: "hosted_file_finalize",
            method: Method::POST,
            path: format!("/files/{file_id}/uploaded"),
            query: None,
            headers: HeaderMap::new(),
            body: Bytes::from_static(b"{}"),
        };
        let response = crate::credentials::control::execute_raw(state, credential_id, operation)
            .await
            .map_err(|e| PipelineError::Transport(e.to_string()))?;
        if !response.status.is_success() {
            return Err(PipelineError::Transport(
                "hosted file finalize failed".into(),
            ));
        }
        let value: Value = serde_json::from_slice(&response.body)
            .map_err(|e| PipelineError::Transport(e.to_string()))?;
        if value.get("status").and_then(Value::as_str) != Some("retry") {
            return Ok(value);
        }
        if attempt == 119 {
            return Err(PipelineError::Transport(
                "hosted file upload not ready".into(),
            ));
        }
        crate::util::time::sleep_ms(250).await;
    }
    unreachable!()
}

async fn save_file_binding(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    credential_id: i64,
    file_id: &str,
    file: Value,
    hosted: Value,
) -> Result<(), PipelineError> {
    state
        .persistence
        .upsert_codex_task_binding(CodexTaskBindingInput {
            provider_id: provider.id,
            task_id: format!("file:{file_id}"),
            credential_id,
            owner_user_id: identity.user.id,
            environment_id: None,
            summary_json: json!({ "file": file, "hosted": hosted, "deleted": false }),
        })
        .await
        .map_err(|e| PipelineError::Transport(e.to_string()))?;
    Ok(())
}

async fn local_task_list(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    query: Option<&str>,
) -> Result<ExecOutcome, PipelineError> {
    let params = query
        .and_then(|query| serde_urlencoded::from_str::<Vec<(String, String)>>(query).ok())
        .unwrap_or_default();
    let value = |name: &str| {
        params
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value.as_str()))
    };
    let limit = value("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let environment = value("environment_id");
    let task_filter = value("task_filter");
    let cursor = value("cursor").and_then(decode_task_cursor);
    let mut rows = state
        .persistence
        .list_codex_task_bindings(provider.id, identity.user.id)
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    rows.retain(|row| !row.task_id.starts_with("file:"));
    rows.sort_by_key(|row| std::cmp::Reverse((row.updated_at, row.id)));
    let mut filtered = rows.into_iter().filter(|row| {
        environment.is_none_or(|environment| row.environment_id.as_deref() == Some(environment))
            && cursor.is_none_or(|(updated_at, id)| (row.updated_at, row.id) < (updated_at, id))
            && task_filter.is_none_or(|filter| match filter {
                "archived" => {
                    row.summary_json.get("archived").and_then(Value::as_bool) == Some(true)
                }
                "current" => {
                    row.summary_json.get("archived").and_then(Value::as_bool) != Some(true)
                }
                _ => true,
            })
    });
    let page = filtered.by_ref().take(limit + 1).collect::<Vec<_>>();
    let has_more = page.len() > limit;
    let items = page
        .iter()
        .take(limit)
        .map(|row| row.summary_json.clone())
        .collect::<Vec<_>>();
    let next = has_more.then(|| {
        let row = &page[limit - 1];
        encode_task_cursor(row.updated_at, row.id)
    });
    Ok(json_outcome(
        StatusCode::OK,
        json!({ "items": items, "cursor": next }),
    ))
}

async fn create_task(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    let mut credentials = {
        let cp = state.cp();
        crate::pipeline::balance::service_credentials(&cp, provider, state.health.as_ref(), None)
    };
    let request_value = serde_json::from_slice::<Value>(&ctx.body).unwrap_or(Value::Null);
    let requested_environment = request_value
        .pointer("/new_task/environment_id")
        .and_then(Value::as_str);
    if let Some(environment_id) = requested_environment {
        let mut ids = environment_credentials(state, provider.id, environment_id).await;
        if ids.is_none() {
            let mut discovery = ctx.clone();
            discovery.method = Method::GET;
            discovery.path = "/api/codex/environments".to_owned();
            discovery.query = None;
            discovery.body = Bytes::new();
            let _ = aggregate_environments(state, provider, &discovery).await;
            ids = environment_credentials(state, provider.id, environment_id).await;
        }
        if let Some(ids) = ids {
            credentials.retain(|credential| ids.contains(&credential.id));
        }
    }
    let credential = credentials.first().ok_or(PipelineError::NoCredentials)?;
    let operation = CredentialControlOperation::CodexRaw {
        label: "task_create",
        method: Method::POST,
        path: "/wham/tasks".to_owned(),
        query: ctx.query.clone(),
        headers: forwarded_headers(&ctx.headers),
        body: ctx.body.clone(),
    };
    let response = crate::credentials::control::execute_raw(state, credential.id, operation)
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    if response.disposition == Disposition::Success {
        let value = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
        let task_id = value
            .pointer("/task/id")
            .or_else(|| value.get("id"))
            .and_then(Value::as_str);
        if let Some(task_id) = task_id {
            let environment_id = request_value
                .pointer("/new_task/environment_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let title = task_title(&request_value);
            let now = unix_now();
            state
                .persistence
                .upsert_codex_task_binding(CodexTaskBindingInput {
                    provider_id: provider.id,
                    task_id: task_id.to_owned(),
                    credential_id: credential.id,
                    owner_user_id: identity.user.id,
                    environment_id,
                    summary_json: json!({
                        "id": task_id,
                        "title": title,
                        "has_generated_title": false,
                        "updated_at": now,
                        "created_at": now,
                        "task_status_display": null,
                        "archived": false,
                        "has_unread_turn": false,
                        "pull_requests": null
                    }),
                })
                .await
                .map_err(|error| PipelineError::Transport(error.to_string()))?;
        } else {
            return Err(PipelineError::Transport(
                "Codex task creation succeeded without a task id".to_owned(),
            ));
        }
    }
    Ok(ExecOutcome {
        status: response.status,
        headers: response.headers,
        body: ResponseBody::Full(response.body),
        disposition: response.disposition,
    })
}

async fn aggregate_environments(
    state: &AppState,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    let credentials = {
        let cp = state.cp();
        crate::pipeline::balance::service_credentials(&cp, provider, state.health.as_ref(), None)
    };
    if credentials.is_empty() {
        return Err(PipelineError::NoCredentials);
    }
    let rest = ctx
        .path
        .strip_prefix("/api/codex/")
        .ok_or(PipelineError::UnsupportedPath)?;
    let mut merged = serde_json::Map::new();
    let mut owners: std::collections::HashMap<String, Vec<i64>> = std::collections::HashMap::new();
    for credential in credentials {
        let operation = CredentialControlOperation::CodexRaw {
            label: "environments",
            method: Method::GET,
            path: format!("/wham/{rest}"),
            query: ctx.query.clone(),
            headers: forwarded_headers(&ctx.headers),
            body: Bytes::new(),
        };
        let Ok(response) =
            crate::credentials::control::execute_raw(state, credential.id, operation).await
        else {
            continue;
        };
        if response.disposition != Disposition::Success {
            continue;
        }
        let Ok(Value::Array(items)) = serde_json::from_slice::<Value>(&response.body) else {
            continue;
        };
        for item in items {
            let Some(id) = item.get("id").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            owners.entry(id.clone()).or_default().push(credential.id);
            merged.entry(id).or_insert(item);
        }
    }
    for (environment_id, credential_ids) in owners {
        let value = credential_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let _ = state
            .cache
            .set(
                &environment_key(provider.id, &environment_id),
                value.into_bytes(),
                Some(std::time::Duration::from_secs(900)),
            )
            .await;
    }
    Ok(json_outcome(
        StatusCode::OK,
        Value::Array(merged.into_iter().map(|(_, value)| value).collect()),
    ))
}

fn environment_key(provider_id: i64, environment_id: &str) -> String {
    format!(
        "codex_env:{provider_id}:{}",
        blake3::hash(environment_id.as_bytes()).to_hex()
    )
}

async fn environment_credentials(
    state: &AppState,
    provider_id: i64,
    environment_id: &str,
) -> Option<Vec<i64>> {
    let value = state
        .cache
        .get(&environment_key(provider_id, environment_id))
        .await?;
    String::from_utf8(value)
        .ok()?
        .split(',')
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

async fn bound_task_request(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
    task_id: &str,
) -> Result<ExecOutcome, PipelineError> {
    let binding = state
        .persistence
        .get_codex_task_binding(provider.id, task_id)
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?
        .filter(|binding| binding.owner_user_id == identity.user.id)
        .ok_or(PipelineError::UnsupportedPath)?;
    let rest = ctx
        .path
        .strip_prefix("/api/codex/")
        .ok_or(PipelineError::UnsupportedPath)?;
    let operation = CredentialControlOperation::CodexRaw {
        label: "task_bound",
        method: ctx.method.clone(),
        path: format!("/wham/{rest}"),
        query: ctx.query.clone(),
        headers: forwarded_headers(&ctx.headers),
        body: ctx.body.clone(),
    };
    let response =
        crate::credentials::control::execute_raw(state, binding.credential_id, operation)
            .await
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
    if ctx.method == Method::GET && response.disposition == Disposition::Success {
        let value = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
        let mut summary = binding.summary_json.clone();
        if let Some(object) = summary.as_object_mut() {
            if let Some(status) = value
                .get("task_status_display")
                .or_else(|| value.pointer("/task/task_status_display"))
            {
                object.insert("task_status_display".to_owned(), status.clone());
            }
            object.insert("updated_at".to_owned(), Value::from(unix_now()));
        }
        let _ = state
            .persistence
            .upsert_codex_task_binding(CodexTaskBindingInput {
                provider_id: binding.provider_id,
                task_id: binding.task_id.clone(),
                credential_id: binding.credential_id,
                owner_user_id: binding.owner_user_id,
                environment_id: binding.environment_id.clone(),
                summary_json: summary,
            })
            .await;
    }
    Ok(ExecOutcome {
        status: response.status,
        headers: response.headers,
        body: ResponseBody::Full(response.body),
        disposition: response.disposition,
    })
}

fn task_id_from_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/codex/tasks/")?;
    let id = rest.split('/').next()?;
    (!id.is_empty() && id != "list").then(|| id.to_owned())
}

fn task_title(request: &Value) -> String {
    request
        .get("input_items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .next()
        .unwrap_or("Codex task")
        .chars()
        .take(120)
        .collect()
}

fn encode_task_cursor(updated_at: i64, id: i64) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{updated_at}:{id}"))
}

fn decode_task_cursor(value: &str) -> Option<(i64, i64)> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()?;
    let value = String::from_utf8(bytes).ok()?;
    let (updated_at, id) = value.split_once(':')?;
    Some((updated_at.parse().ok()?, id.parse().ok()?))
}

fn service_name(path: &str) -> &str {
    if path.ends_with("/whoami") {
        return "whoami";
    }
    if path == "/v1/files" || path.starts_with("/v1/files/") {
        return "files";
    }
    if path.contains("/memories/") {
        return "memories";
    }
    path.strip_prefix("/api/codex/")
        .or_else(|| path.strip_prefix("/backend-api/"))
        .and_then(|rest| rest.split('/').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("service")
}

fn plan_type(provider: &Provider) -> &str {
    provider
        .settings_json
        .get("codex_pat_plan_type")
        .and_then(Value::as_str)
        .filter(|value| {
            matches!(
                *value,
                "free" | "go" | "plus" | "pro" | "team" | "business" | "enterprise" | "edu"
            )
        })
        .unwrap_or("pro")
}

fn stable_id(kind: &str, provider_id: i64, user_id: i64) -> String {
    let value = format!("gproxy-codex-{kind}:{provider_id}:{user_id}");
    let hash = blake3::hash(value.as_bytes()).to_hex();
    format!("gproxy-{kind}-{}", &hash.as_str()[..24])
}

fn virtual_ids(
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Provider,
) -> (String, String) {
    (
        stable_id("user", provider.id, identity.user.id),
        stable_id("account", provider.id, identity.user.id),
    )
}

fn whoami(identity: &crate::app::snapshot::KeyIdentity, provider: &Provider) -> Value {
    let (user_id, account_id) = virtual_ids(identity, provider);
    let email = identity
        .user
        .name
        .contains('@')
        .then(|| identity.user.name.clone());
    json!({
        "email": email,
        "chatgpt_user_id": user_id,
        "chatgpt_account_id": account_id,
        "chatgpt_plan_type": plan_type(provider),
        "chatgpt_account_is_fedramp": false
    })
}

fn virtual_account(identity: &crate::app::snapshot::KeyIdentity, provider: &Provider) -> Value {
    let (_, account_id) = virtual_ids(identity, provider);
    json!({
        "accounts": [{
            "id": account_id.clone(),
            "name": identity.user.name.clone(),
            "profile_picture_url": null,
            "structure": plan_type(provider)
        }],
        "account_ordering": [account_id.clone()],
        "default_account_id": account_id
    })
}

async fn virtual_profile(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Provider,
) -> Result<ExecOutcome, PipelineError> {
    let summary = state
        .persistence
        .summarize_usages(&crate::store::persistence::UsageQuery {
            provider_id: Some(provider.id),
            user_id: Some(identity.user.id),
            ..Default::default()
        })
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    let lifetime = summary.input_tokens.saturating_add(summary.output_tokens);
    Ok(json_outcome(
        StatusCode::OK,
        json!({
            "stats": {
                "lifetime_tokens": lifetime,
                "peak_daily_tokens": null,
                "longest_running_turn_sec": null,
                "current_streak_days": null,
                "longest_streak_days": null,
                "daily_usage_buckets": null
            }
        }),
    ))
}

async fn virtual_usage(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Provider,
) -> Value {
    let table = state.quotas.current(state.persistence.as_ref()).await;
    let scopes = [
        Some((Scope::User, identity.user.id)),
        identity.user.team_id.map(|id| (Scope::Team, id)),
        Some((Scope::Org, identity.user.org_id)),
    ];
    let quotas = scopes
        .into_iter()
        .flatten()
        .filter_map(|scope| table.get(&scope).map(Arc::as_ref))
        .collect::<Vec<_>>();
    let now = unix_now();
    let primary = strict_window(&quotas, now, true);
    let secondary = strict_window(&quotas, now, false);
    let reached = [primary.as_ref(), secondary.as_ref()]
        .into_iter()
        .flatten()
        .any(|window| {
            window
                .get("used_percent")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                >= 100
        });
    json!({
        "plan_type": plan_type(provider),
        "rate_limit": {
            "allowed": !reached,
            "limit_reached": reached,
            "primary_window": primary,
            "secondary_window": secondary
        },
        "credits": null,
        "spend_control": null,
        "additional_rate_limits": null,
        "rate_limit_reached_type": if reached { json!({ "type": "rate_limit_reached" }) } else { Value::Null },
        "rate_limit_reset_credits": { "available_count": 0 }
    })
}

fn strict_window(quotas: &[&Quota], now: i64, five_hour: bool) -> Option<Value> {
    let duration = if five_hour {
        crate::util::timewindow::FIVE_HOURS_SECS
    } else {
        crate::util::timewindow::SEVEN_DAYS_SECS
    };
    quotas
        .iter()
        .filter_map(|quota| {
            let (limit, used, anchor) = if five_hour {
                (
                    quota.quota_5h?,
                    quota.five_hour_used,
                    quota.five_hour_anchor,
                )
            } else {
                (
                    quota.quota_7d?,
                    quota.seven_day_used,
                    quota.seven_day_anchor,
                )
            };
            let used = crate::util::timewindow::anchored_used(anchor, used, now, duration);
            let limit_f = limit.to_string().parse::<f64>().ok()?;
            let used_f = used.to_string().parse::<f64>().ok()?;
            let percent = if limit_f <= 0.0 {
                100
            } else {
                ((used_f / limit_f) * 100.0).clamp(0.0, 100.0).round() as i64
            };
            let effective_anchor = if anchor > 0 { anchor } else { now };
            let reset_at = effective_anchor.saturating_add(duration);
            Some((percent, reset_at))
        })
        .max_by_key(|(percent, _)| *percent)
        .map(|(percent, reset_at)| {
            json!({
                "used_percent": percent,
                "limit_window_seconds": duration,
                "reset_after_seconds": reset_at.saturating_sub(now),
                "reset_at": reset_at
            })
        })
}

fn allowlisted_upstream(method: &Method, path: &str) -> Option<(&'static str, String)> {
    if method == Method::POST && path == "/v1/memories/trace_summarize" {
        return Some(("memories", "/codex/memories/trace_summarize".to_owned()));
    }
    let rest = path.strip_prefix("/api/codex/")?;
    let allowed = match (method.as_str(), rest) {
        ("GET", "environments") => ("environments", "/wham/environments".to_owned()),
        ("GET", value) if value.starts_with("environments/by-repo/") => {
            ("environments_by_repo", format!("/wham/{value}"))
        }
        ("GET" | "POST", value)
            if value == "tasks" || value.starts_with("tasks/") || value == "tasks/list" =>
        {
            ("tasks", format!("/wham/{value}"))
        }
        ("POST", "ps/mcp") => ("ps_mcp", "/ps/mcp".to_owned()),
        ("GET" | "POST" | "DELETE", value)
            if value.starts_with("ps/plugins/") || value.starts_with("ps/apps/") =>
        {
            ("plugins", format!("/{value}"))
        }
        ("GET" | "POST", value) if value.starts_with("agent-identities/") => {
            ("agent_identity", format!("/wham/{value}"))
        }
        ("GET" | "POST" | "DELETE", value)
            if value.starts_with("remote/control/") || value == "remote/control" =>
        {
            ("remote_control", format!("/wham/{value}"))
        }
        ("POST", value)
            if matches!(
                value,
                "responses"
                    | "responses/compact"
                    | "images/generations"
                    | "images/edits"
                    | "alpha/search"
                    | "memories/trace_summarize"
                    | "realtime/calls"
            ) =>
        {
            ("codex_model_api", format!("/codex/{value}"))
        }
        ("GET", value) if value == "models" || value.starts_with("models/") => {
            ("codex_models", format!("/codex/{value}"))
        }
        _ => return None,
    };
    Some(allowed)
}

fn forwarded_headers(input: &HeaderMap) -> HeaderMap {
    let mut output = HeaderMap::new();
    for name in [
        "accept",
        "content-type",
        "cache-control",
        "mcp-session-id",
        "last-event-id",
        "x-codex-turn-metadata",
        "x-codex-installation-id",
        "x-client-request-id",
    ] {
        if let Some(value) = input.get(name) {
            output.insert(http::HeaderName::from_static(name), value.clone());
        }
    }
    for (name, value) in input {
        if name.as_str().starts_with("x-codex-") {
            output.insert(name.clone(), value.clone());
        }
    }
    output
}

fn remote_control_token_key(provider_id: i64, token: &str) -> String {
    format!(
        "codex_rc_token:{provider_id}:{}",
        blake3::hash(token.as_bytes()).to_hex()
    )
}

fn is_retryable(method: &Method, path: &str) -> bool {
    method == Method::GET || path.ends_with("/list") || path.ends_with("/query")
}

async fn execute_balanced(
    state: &AppState,
    provider: &Arc<Provider>,
    operation: CredentialControlOperation,
    retryable: bool,
    user_id: i64,
) -> Result<ExecOutcome, PipelineError> {
    let mcp_session = match &operation {
        CredentialControlOperation::CodexRaw { label, headers, .. } if *label == "ps_mcp" => {
            headers
                .get("mcp-session-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        }
        _ => None,
    };
    let binding_key = mcp_session.as_deref().map(|session| {
        format!(
            "codex_mcp:{}:{user_id}:{}",
            provider.id,
            blake3::hash(session.as_bytes()).to_hex()
        )
    });
    let remote_server = match &operation {
        CredentialControlOperation::CodexRaw {
            label,
            body,
            headers,
            ..
        } if *label == "remote_control" => serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("server_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(|| {
                headers
                    .get("x-codex-server-id")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            }),
        _ => None,
    };
    let remote_binding_key = remote_server.as_deref().map(|server_id| {
        format!(
            "codex_rc_server:{}:{user_id}:{}",
            provider.id,
            blake3::hash(server_id.as_bytes()).to_hex()
        )
    });
    let plugin_id = match &operation {
        CredentialControlOperation::CodexRaw { label, path, .. } if *label == "plugins" => path
            .strip_prefix("/ps/plugins/")
            .and_then(|rest| rest.split('/').next())
            .filter(|id| {
                !matches!(
                    *id,
                    "list" | "installed" | "search" | "suggested" | "workspace"
                )
            })
            .map(str::to_owned),
        _ => None,
    };
    let plugin_binding_key = plugin_id.as_deref().map(|plugin_id| {
        format!(
            "codex_plugin:{}:{user_id}:{}",
            provider.id,
            blake3::hash(plugin_id.as_bytes()).to_hex()
        )
    });
    let pinned = match binding_key
        .as_deref()
        .or(remote_binding_key.as_deref())
        .or(plugin_binding_key.as_deref())
    {
        Some(key) => state
            .cache
            .get(key)
            .await
            .and_then(|value| String::from_utf8(value).ok())
            .and_then(|value| value.parse().ok()),
        None => None,
    };
    let credentials = {
        let cp = state.cp();
        crate::pipeline::balance::service_credentials(&cp, provider, state.health.as_ref(), pinned)
    };
    if credentials.is_empty() {
        return Err(PipelineError::NoCredentials);
    }
    let is_mcp = matches!(&operation, CredentialControlOperation::CodexRaw { label, .. } if *label == "ps_mcp");
    let is_model_stream = match &operation {
        CredentialControlOperation::CodexRaw { label, body, .. } if *label == "codex_model_api" => {
            serde_json::from_slice::<Value>(body)
                .ok()
                .and_then(|value| value.get("stream").and_then(Value::as_bool))
                .unwrap_or(false)
        }
        _ => false,
    };
    if is_mcp || is_model_stream {
        let credential = credentials.first().ok_or(PipelineError::NoCredentials)?;
        let response =
            crate::credentials::control::execute_raw_streaming(state, credential.id, operation)
                .await
                .map_err(|error| PipelineError::Transport(error.to_string()))?;
        if response.disposition == Disposition::Success
            && let Some(session) = response
                .headers
                .get("mcp-session-id")
                .and_then(|value| value.to_str().ok())
        {
            let key = format!(
                "codex_mcp:{}:{user_id}:{}",
                provider.id,
                blake3::hash(session.as_bytes()).to_hex()
            );
            let _ = state
                .cache
                .set(
                    &key,
                    credential.id.to_string().into_bytes(),
                    Some(std::time::Duration::from_secs(24 * 3600)),
                )
                .await;
        }
        return Ok(ExecOutcome {
            status: response.status,
            headers: response.headers,
            body: ResponseBody::Stream(response.body),
            disposition: response.disposition,
        });
    }
    let mut last_error = None;
    for credential in credentials {
        match crate::credentials::control::execute_raw(state, credential.id, operation.clone())
            .await
        {
            Ok(response) => {
                if response.disposition == Disposition::Success
                    && matches!(&operation, CredentialControlOperation::CodexRaw { label, .. } if *label == "remote_control")
                    && let Ok(value) = serde_json::from_slice::<Value>(&response.body)
                    && let Some(token) = value.get("remote_control_token").and_then(Value::as_str)
                {
                    let key = remote_control_token_key(provider.id, token);
                    let _ = state
                        .cache
                        .set(
                            &key,
                            format!("{}:{user_id}", credential.id).into_bytes(),
                            Some(std::time::Duration::from_secs(7 * 86_400)),
                        )
                        .await;
                }
                if response.disposition == Disposition::Success
                    && let Some(key) = plugin_binding_key.as_deref()
                {
                    let _ = state
                        .cache
                        .set(
                            key,
                            credential.id.to_string().into_bytes(),
                            Some(std::time::Duration::from_secs(30 * 86_400)),
                        )
                        .await;
                }
                if response.disposition == Disposition::Success
                    && matches!(&operation, CredentialControlOperation::CodexRaw { label, .. } if *label == "remote_control")
                    && let Ok(value) = serde_json::from_slice::<Value>(&response.body)
                    && let Some(server_id) = value.get("server_id").and_then(Value::as_str)
                {
                    let key = format!(
                        "codex_rc_server:{}:{user_id}:{}",
                        provider.id,
                        blake3::hash(server_id.as_bytes()).to_hex()
                    );
                    let _ = state
                        .cache
                        .set(
                            &key,
                            credential.id.to_string().into_bytes(),
                            Some(std::time::Duration::from_secs(7 * 86_400)),
                        )
                        .await;
                }
                if response.disposition == Disposition::Success
                    && matches!(&operation, CredentialControlOperation::CodexRaw { label, .. } if *label == "ps_mcp")
                    && let Some(session) = response
                        .headers
                        .get("mcp-session-id")
                        .and_then(|value| value.to_str().ok())
                {
                    let key = format!(
                        "codex_mcp:{}:{user_id}:{}",
                        provider.id,
                        blake3::hash(session.as_bytes()).to_hex()
                    );
                    let _ = state
                        .cache
                        .set(
                            &key,
                            credential.id.to_string().into_bytes(),
                            Some(std::time::Duration::from_secs(24 * 3600)),
                        )
                        .await;
                }
                if response.disposition == Disposition::Success
                    || !retryable
                    || !response.disposition.should_failover()
                {
                    return Ok(ExecOutcome {
                        status: response.status,
                        headers: response.headers,
                        body: ResponseBody::Full(response.body),
                        disposition: response.disposition,
                    });
                }
            }
            Err(error) => {
                last_error = Some(error.to_string());
                if !retryable {
                    break;
                }
            }
        }
    }
    Err(PipelineError::Transport(last_error.unwrap_or_else(|| {
        "all Codex service credentials failed".to_owned()
    })))
}

fn json_outcome(status: StatusCode, value: Value) -> ExecOutcome {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Full(Bytes::from(serde_json::to_vec(&value).unwrap_or_default())),
        disposition: Disposition::Success,
    }
}
