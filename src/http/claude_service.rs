//! Scoped Claude Code API-key compatibility and stateful Claude resources.

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::channel::{CredentialControlOperation, Disposition};
use crate::pipeline::context::{RequestCtx, RoutingMode};
use crate::pipeline::error::PipelineError;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::store::persistence::records::{CodexTaskBindingInput, Credential, Provider};
use crate::util::time::unix_now;

const FILE_PREFIX: &str = "claude:file:";
const SKILL_PREFIX: &str = "claude:skill:";

pub async fn execute(
    state: &AppState,
    mut ctx: RequestCtx,
) -> Option<Result<ExecOutcome, PipelineError>> {
    if !is_service_path(&ctx.path) {
        return None;
    }
    let provider_name = match &ctx.mode {
        RoutingMode::Named { name } | RoutingMode::Scoped { provider: name } => name,
        _ => return Some(Err(PipelineError::UnsupportedPath)),
    };
    let cp = state.cp();
    let provider = match cp
        .providers_by_name
        .get(provider_name)
        .filter(|provider| provider.enabled && provider.channel == "claudecode")
        .cloned()
    {
        Some(provider) => provider,
        None => return None,
    };
    let result = async {
        let identity =
            crate::pipeline::auth::authenticate(&cp, &ctx.headers, ctx.query.as_deref())?;
        let service = service_name(&ctx.path);
        let authorization = crate::pipeline::authz::prepare_provider_service_namespace(
            &cp,
            &identity,
            &provider.name,
            "claude",
            service,
        )?;
        drop(cp);
        crate::pipeline::authz::authorize(&authorization, state.cache.as_ref(), unix_now()).await?;
        ctx.identity = Some(identity.clone());
        run(state, &identity, &provider, &ctx).await
    }
    .await;
    Some(result)
}

fn is_service_path(path: &str) -> bool {
    path == "/api/hello"
        || path == "/api/claude_cli/bootstrap"
        || path == "/api/claude_cli_profile"
        || path == "/api/claude_code_penguin_mode"
        || path == "/api/claude_code/skills"
        || path == "/api/oauth/file_upload"
        || path.starts_with("/api/oauth/files/")
        || path.starts_with("/api/oauth/organizations/")
        || path == "/v1/files"
        || path.starts_with("/v1/files/")
        || path == "/v1/skills"
        || path.starts_with("/v1/skills/")
}

fn service_name(path: &str) -> &str {
    if path.contains("skill") {
        "skills"
    } else if path.contains("file") {
        "files"
    } else if path.contains("bootstrap") {
        "bootstrap"
    } else if path.contains("profile") {
        "profile"
    } else {
        "service"
    }
}

async fn run(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    match (ctx.method.as_str(), ctx.path.as_str()) {
        ("GET", "/api/hello") => return Ok(json_outcome(StatusCode::OK, json!({}))),
        ("GET", "/api/claude_cli/bootstrap") => {
            return Ok(json_outcome(
                StatusCode::OK,
                bootstrap(state, identity, provider),
            ));
        }
        ("GET", "/api/claude_cli_profile") => {
            return Ok(json_outcome(StatusCode::OK, profile(identity, provider)));
        }
        ("GET", "/api/claude_code_penguin_mode") => {
            return Ok(json_outcome(StatusCode::OK, penguin(provider)));
        }
        ("GET", "/api/claude_code/skills") => {
            return Ok(json_outcome(
                StatusCode::OK,
                json!({ "skills": provider.settings_json.get("claudecode_skill_health").and_then(Value::as_array).cloned().unwrap_or_default() }),
            ));
        }
        ("POST", "/api/oauth/file_upload") => {
            return oauth_file_upload(state, identity, provider, ctx).await;
        }
        _ => {}
    }

    if let Some(file_id) = ctx
        .path
        .strip_prefix("/api/oauth/files/")
        .and_then(|rest| rest.strip_suffix("/content"))
        .filter(|id| !id.is_empty() && !id.contains('/'))
    {
        if ctx.method != Method::GET {
            return Err(PipelineError::UnsupportedPath);
        }
        return file_content(state, identity, provider, file_id).await;
    }

    if let Some((organization, action)) = oauth_skill_action(&ctx.path) {
        if organization != stable_id("organization", provider.id, identity.user.id) {
            return Err(PipelineError::UnsupportedPath);
        }
        return oauth_skills(state, identity, provider, ctx, action).await;
    }
    if ctx.path == "/v1/files" || ctx.path.starts_with("/v1/files/") {
        return official_files(state, identity, provider, ctx).await;
    }
    if ctx.path == "/v1/skills" || ctx.path.starts_with("/v1/skills/") {
        return official_skills(state, identity, provider, ctx).await;
    }
    Err(PipelineError::UnsupportedPath)
}

fn stable_id(kind: &str, provider_id: i64, user_id: i64) -> String {
    let hash = blake3::hash(format!("gproxy-claude-{kind}:{provider_id}:{user_id}").as_bytes());
    format!("gproxy-{kind}-{}", &hash.to_hex().as_str()[..24])
}

fn profile(identity: &crate::app::snapshot::KeyIdentity, provider: &Provider) -> Value {
    json!({
        "account": {
            "uuid": stable_id("account", provider.id, identity.user.id),
            "email": format!("{}@gproxy.invalid", identity.user.name),
            "display_name": identity.user.name,
        },
        "organization": {
            "uuid": stable_id("organization", provider.id, identity.user.id),
            "organization_type": "api",
            "rate_limit_tier": "gproxy",
        }
    })
}

fn bootstrap(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Provider,
) -> Value {
    if let Some(value) = provider.settings_json.get("claudecode_bootstrap") {
        return value.clone();
    }
    let cp = state.cp();
    let models = cp
        .exposed_models_by_provider
        .get(&provider.id)
        .map(|models| models.as_ref().as_slice())
        .unwrap_or_default();
    let options = models
        .iter()
        .map(|model| {
            json!({
                "model": model.full_id,
                "name": model.display_name.as_deref().unwrap_or(&model.full_id),
                "description": "",
                "disabled_reason": null,
            })
        })
        .collect::<Vec<_>>();
    let access = models
        .iter()
        .map(|model| json!({
            "api_name": model.full_id,
            "entitled": true,
            "max_effort_level": if model.thinking_supported == Some(true) { "high" } else { "medium" },
        }))
        .collect::<Vec<_>>();
    let compact = models
        .iter()
        .filter_map(|model| {
            model
                .context_window
                .map(|window| (model.full_id.clone(), json!((window * 9 / 10).max(1))))
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "client_data": {},
        "additional_model_options": options,
        "additional_model_costs": {},
        "model_access": access,
        "org_model_default": null,
        "oauth_account": {
            "account_uuid": stable_id("account", provider.id, identity.user.id),
            "account_email": format!("{}@gproxy.invalid", identity.user.name),
            "organization_uuid": stable_id("organization", provider.id, identity.user.id),
            "organization_name": provider.label.as_deref().unwrap_or(&provider.name),
            "organization_type": "api",
            "organization_rate_limit_tier": "gproxy",
        },
        "auto_compact_windows": compact,
        "narrowed": false,
    })
}

fn penguin(provider: &Provider) -> Value {
    provider
        .settings_json
        .get("claudecode_fast_mode")
        .cloned()
        .unwrap_or_else(|| json!({ "enabled": false, "disabled_reason": "preference" }))
}

async fn oauth_file_upload(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    let credential = choose_credential(state, provider)?;
    let response = raw(
        state,
        credential.id,
        "claude_oauth_file_upload",
        Method::POST,
        "/v1/files",
        None,
        resource_headers(&ctx.headers, "files-api-2025-04-14"),
        ctx.body.clone(),
    )
    .await?;
    if !response.status.is_success() {
        return Ok(raw_outcome(response));
    }
    let value: Value = serde_json::from_slice(&response.body)
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| PipelineError::Transport("Claude file response missing id".into()))?
        .to_owned();
    save_binding(
        state,
        identity,
        provider,
        credential.id,
        FILE_PREFIX,
        &id,
        value,
    )
    .await?;
    Ok(json_outcome(
        StatusCode::CREATED,
        json!({ "file_uuid": id }),
    ))
}

async fn official_files(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    if ctx.path == "/v1/files" {
        if ctx.method == Method::POST {
            let credential = choose_credential(state, provider)?;
            let response = raw(
                state,
                credential.id,
                "claude_file_create",
                Method::POST,
                "/v1/files",
                safe_query(ctx.query.as_deref()),
                resource_headers(&ctx.headers, "files-api-2025-04-14"),
                ctx.body.clone(),
            )
            .await?;
            if response.status.is_success() {
                let value: Value = serde_json::from_slice(&response.body)
                    .map_err(|error| PipelineError::Transport(error.to_string()))?;
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    save_binding(
                        state,
                        identity,
                        provider,
                        credential.id,
                        FILE_PREFIX,
                        id,
                        value.clone(),
                    )
                    .await?;
                }
            }
            return Ok(raw_outcome(response));
        }
        if ctx.method == Method::GET {
            return local_list(state, identity, provider, FILE_PREFIX, &ctx.query).await;
        }
        return Err(PipelineError::UnsupportedPath);
    }
    let rest = ctx
        .path
        .strip_prefix("/v1/files/")
        .ok_or(PipelineError::UnsupportedPath)?;
    let (id, content) = rest
        .strip_suffix("/content")
        .map(|id| (id, true))
        .unwrap_or((rest, false));
    if id.is_empty() || id.contains('/') {
        return Err(PipelineError::UnsupportedPath);
    }
    if content {
        if ctx.method != Method::GET {
            return Err(PipelineError::UnsupportedPath);
        }
        return file_content(state, identity, provider, id).await;
    }
    let binding = owned_binding(state, identity, provider, FILE_PREFIX, id).await?;
    match ctx.method.as_str() {
        "GET" => {
            forward_bound(
                state,
                &binding,
                "claude_file_retrieve",
                Method::GET,
                &format!("/v1/files/{id}"),
                safe_query(ctx.query.as_deref()),
                resource_headers(&ctx.headers, "files-api-2025-04-14"),
                Bytes::new(),
            )
            .await
        }
        "DELETE" => {
            let outcome = forward_bound(
                state,
                &binding,
                "claude_file_delete",
                Method::DELETE,
                &format!("/v1/files/{id}"),
                safe_query(ctx.query.as_deref()),
                resource_headers(&ctx.headers, "files-api-2025-04-14"),
                Bytes::new(),
            )
            .await?;
            if outcome.status.is_success() {
                mark_deleted(state, binding).await?;
            }
            Ok(outcome)
        }
        _ => Err(PipelineError::UnsupportedPath),
    }
}

async fn file_content(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    id: &str,
) -> Result<ExecOutcome, PipelineError> {
    let binding = owned_binding(state, identity, provider, FILE_PREFIX, id).await?;
    let mut headers = resource_headers(&HeaderMap::new(), "files-api-2025-04-14");
    headers.insert(
        http::header::ACCEPT,
        http::HeaderValue::from_static("application/octet-stream"),
    );
    let operation = CredentialControlOperation::ClaudeRaw {
        label: "claude_file_content",
        method: Method::GET,
        path: format!("/v1/files/{id}/content"),
        query: None,
        headers,
        body: Bytes::new(),
    };
    let response =
        crate::credentials::control::execute_raw_streaming(state, binding.credential_id, operation)
            .await
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
    Ok(ExecOutcome {
        status: response.status,
        headers: response.headers,
        body: ResponseBody::Stream(response.body),
        disposition: response.disposition,
    })
}

#[derive(Clone, Copy)]
enum OauthSkillAction {
    List,
    Search,
    Download,
}

fn oauth_skill_action(path: &str) -> Option<(&str, OauthSkillAction)> {
    let rest = path.strip_prefix("/api/oauth/organizations/")?;
    let (organization, rest) = rest.split_once("/skills/")?;
    if rest == "list-skills" {
        Some((organization, OauthSkillAction::List))
    } else if rest == "search" {
        Some((organization, OauthSkillAction::Search))
    } else if rest.ends_with("/download") {
        Some((organization, OauthSkillAction::Download))
    } else {
        None
    }
}

async fn oauth_skills(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
    action: OauthSkillAction,
) -> Result<ExecOutcome, PipelineError> {
    match action {
        OauthSkillAction::Download => {
            if ctx.method != Method::GET {
                return Err(PipelineError::UnsupportedPath);
            }
            Ok(json_outcome(
                StatusCode::NOT_FOUND,
                json!({ "error": { "type": "not_found_error", "message": "skill archive unavailable" } }),
            ))
        }
        OauthSkillAction::List | OauthSkillAction::Search => {
            let mut skills = skill_catalog(state, identity, provider).await?;
            if matches!(action, OauthSkillAction::Search) {
                if ctx.method != Method::POST {
                    return Err(PipelineError::UnsupportedPath);
                }
                let keywords = serde_json::from_slice::<Value>(&ctx.body)
                    .ok()
                    .and_then(|value| value.get("keywords").and_then(Value::as_array).cloned())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|value| value.as_str().map(|value| value.to_ascii_lowercase()))
                    .collect::<Vec<_>>();
                if !keywords.is_empty() {
                    skills.retain(|skill| {
                        let haystack = format!(
                            "{} {}",
                            skill.get("name").and_then(Value::as_str).unwrap_or(""),
                            skill
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                        )
                        .to_ascii_lowercase();
                        keywords.iter().any(|keyword| haystack.contains(keyword))
                    });
                }
                Ok(json_outcome(StatusCode::OK, json!({ "results": skills })))
            } else {
                if ctx.method != Method::GET {
                    return Err(PipelineError::UnsupportedPath);
                }
                Ok(json_outcome(StatusCode::OK, json!({ "skills": skills })))
            }
        }
    }
}

async fn official_skills(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    if ctx.path == "/v1/skills" {
        if ctx.method == Method::GET {
            return local_list(state, identity, provider, SKILL_PREFIX, &ctx.query).await;
        }
        if ctx.method == Method::POST {
            let credential = choose_credential(state, provider)?;
            let response = raw(
                state,
                credential.id,
                "claude_skill_create",
                Method::POST,
                "/v1/skills",
                Some(merge_query(
                    safe_query(ctx.query.as_deref()).as_deref(),
                    "beta=true",
                )),
                resource_headers(&ctx.headers, "skills-2025-10-02"),
                ctx.body.clone(),
            )
            .await?;
            if response.status.is_success() {
                let value: Value = serde_json::from_slice(&response.body)
                    .map_err(|error| PipelineError::Transport(error.to_string()))?;
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    save_binding(
                        state,
                        identity,
                        provider,
                        credential.id,
                        SKILL_PREFIX,
                        id,
                        value.clone(),
                    )
                    .await?;
                }
            }
            return Ok(raw_outcome(response));
        }
        return Err(PipelineError::UnsupportedPath);
    }
    let rest = ctx
        .path
        .strip_prefix("/v1/skills/")
        .ok_or(PipelineError::UnsupportedPath)?;
    let skill_id = rest
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
        .ok_or(PipelineError::UnsupportedPath)?;
    let binding = owned_binding(state, identity, provider, SKILL_PREFIX, skill_id).await?;
    let method = ctx.method.clone();
    let allowed = if rest == skill_id {
        method == Method::GET || method == Method::DELETE
    } else if rest == format!("{skill_id}/versions") {
        method == Method::GET || method == Method::POST
    } else if rest
        .strip_prefix(&format!("{skill_id}/versions/"))
        .is_some_and(|version| !version.is_empty() && !version.contains('/'))
    {
        method == Method::GET || method == Method::DELETE
    } else {
        false
    };
    if !allowed {
        return Err(PipelineError::UnsupportedPath);
    }
    let response = raw(
        state,
        binding.credential_id,
        "claude_skill_resource",
        method.clone(),
        &ctx.path,
        Some(merge_query(
            safe_query(ctx.query.as_deref()).as_deref(),
            "beta=true",
        )),
        resource_headers(&ctx.headers, "skills-2025-10-02"),
        ctx.body.clone(),
    )
    .await?;
    if method == Method::POST
        && rest == format!("{skill_id}/versions")
        && response.status.is_success()
        && let Ok(version) = serde_json::from_slice::<Value>(&response.body)
    {
        let mut summary = binding.summary_json.clone();
        if let Some(description) = version.get("description") {
            summary["resource"]["description"] = description.clone();
        }
        if let Some(version_id) = version.get("id") {
            summary["resource"]["latest_version_id"] = version_id.clone();
        }
        state
            .persistence
            .upsert_codex_task_binding(CodexTaskBindingInput {
                provider_id: binding.provider_id,
                task_id: binding.task_id.clone(),
                credential_id: binding.credential_id,
                owner_user_id: binding.owner_user_id,
                environment_id: binding.environment_id.clone(),
                summary_json: summary,
            })
            .await
            .map_err(|error| PipelineError::Transport(error.to_string()))?;
    }
    let outcome = raw_outcome(response);
    if method == Method::DELETE && rest == skill_id && outcome.status.is_success() {
        mark_deleted(state, binding).await?;
    }
    Ok(outcome)
}

fn merge_query(existing: Option<&str>, required: &str) -> String {
    if existing.is_some_and(|query| query.split('&').any(|part| part == required)) {
        return existing.unwrap_or_default().to_owned();
    }
    match existing.filter(|query| !query.is_empty()) {
        Some(query) => format!("{required}&{query}"),
        None => required.to_owned(),
    }
}

fn safe_query(query: Option<&str>) -> Option<String> {
    let kept = query?
        .split('&')
        .filter(|part| part.split('=').next() != Some("key"))
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

async fn skill_catalog(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
) -> Result<Vec<Value>, PipelineError> {
    let mut rows = state
        .persistence
        .list_codex_task_bindings(provider.id, identity.user.id)
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    rows.sort_by_key(|row| std::cmp::Reverse((row.updated_at, row.id)));
    let mut skills = rows
        .into_iter()
        .filter(|row| {
            row.task_id.starts_with(SKILL_PREFIX)
                && row.summary_json.get("deleted").and_then(Value::as_bool) != Some(true)
        })
        .filter_map(|row| row.summary_json.get("resource").map(skill_to_oauth))
        .collect::<Vec<_>>();
    skills.extend(
        provider
            .settings_json
            .get("claudecode_shared_skills")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    );
    Ok(skills)
}

fn skill_to_oauth(skill: &Value) -> Value {
    json!({
        "id": skill.get("id").cloned().unwrap_or(Value::Null),
        "name": skill.get("display_name").cloned().unwrap_or(Value::Null),
        "description": skill.get("description").cloned().unwrap_or(Value::Null),
        "source": skill.pointer("/source/type").cloned().unwrap_or_else(|| json!("custom")),
        "updated_at": skill.get("updated_at").cloned().unwrap_or(Value::Null),
        "enabled": true,
    })
}

async fn local_list(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    prefix: &str,
    query: &Option<String>,
) -> Result<ExecOutcome, PipelineError> {
    let mut rows = state
        .persistence
        .list_codex_task_bindings(provider.id, identity.user.id)
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    rows.sort_by_key(|row| std::cmp::Reverse((row.updated_at, row.id)));
    let mut data = rows
        .into_iter()
        .filter(|row| {
            row.task_id.starts_with(prefix)
                && row.summary_json.get("deleted").and_then(Value::as_bool) != Some(true)
        })
        .filter_map(|row| row.summary_json.get("resource").cloned())
        .collect::<Vec<_>>();
    let params = query
        .as_deref()
        .and_then(|query| serde_urlencoded::from_str::<Vec<(String, String)>>(query).ok())
        .unwrap_or_default();
    let ids = params
        .iter()
        .filter_map(|(key, value)| (key == "ids[]").then_some(value.as_str()))
        .collect::<Vec<_>>();
    if !ids.is_empty() {
        data.retain(|resource| {
            resource
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| ids.contains(&id))
        });
    }
    if let Some(source) = query_value(&params, "source") {
        data.retain(|resource| {
            resource.pointer("/source/type").and_then(Value::as_str) == Some(source)
        });
    }
    let offset = query_value(&params, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query_value(&params, "limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 1000);
    let has_more = data.len() > offset.saturating_add(limit);
    let data = data
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next_page = has_more.then(|| offset.saturating_add(limit).to_string());
    Ok(json_outcome(
        StatusCode::OK,
        json!({ "data": data, "next_page": next_page }),
    ))
}

fn query_value<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value.as_str()))
}

fn choose_credential(
    state: &AppState,
    provider: &Arc<Provider>,
) -> Result<Arc<Credential>, PipelineError> {
    let credentials = {
        let cp = state.cp();
        crate::pipeline::balance::service_credentials(&cp, provider, state.health.as_ref(), None)
    };
    credentials
        .first()
        .cloned()
        .ok_or(PipelineError::NoCredentials)
}

async fn owned_binding(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    prefix: &str,
    id: &str,
) -> Result<crate::store::persistence::records::CodexTaskBinding, PipelineError> {
    state
        .persistence
        .get_codex_task_binding(provider.id, &format!("{prefix}{id}"))
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?
        .filter(|binding| {
            binding.owner_user_id == identity.user.id
                && binding.summary_json.get("deleted").and_then(Value::as_bool) != Some(true)
        })
        .ok_or(PipelineError::UnsupportedPath)
}

async fn save_binding(
    state: &AppState,
    identity: &crate::app::snapshot::KeyIdentity,
    provider: &Arc<Provider>,
    credential_id: i64,
    prefix: &str,
    id: &str,
    resource: Value,
) -> Result<(), PipelineError> {
    state
        .persistence
        .upsert_codex_task_binding(CodexTaskBindingInput {
            provider_id: provider.id,
            task_id: format!("{prefix}{id}"),
            credential_id,
            owner_user_id: identity.user.id,
            environment_id: None,
            summary_json: json!({ "resource": resource, "deleted": false }),
        })
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    Ok(())
}

async fn mark_deleted(
    state: &AppState,
    binding: crate::store::persistence::records::CodexTaskBinding,
) -> Result<(), PipelineError> {
    let mut summary = binding.summary_json;
    summary["deleted"] = Value::Bool(true);
    state
        .persistence
        .upsert_codex_task_binding(CodexTaskBindingInput {
            provider_id: binding.provider_id,
            task_id: binding.task_id,
            credential_id: binding.credential_id,
            owner_user_id: binding.owner_user_id,
            environment_id: binding.environment_id,
            summary_json: summary,
        })
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    Ok(())
}

fn resource_headers(input: &HeaderMap, beta: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in [http::header::CONTENT_TYPE, http::header::ACCEPT] {
        if let Some(value) = input.get(&name) {
            headers.insert(name, value.clone());
        }
    }
    if let Some(value) = input.get("anthropic-beta") {
        headers.insert("anthropic-beta", value.clone());
    }
    crate::channel::shaping::anthropic_beta::append_beta_token(&mut headers, beta);
    headers
}

async fn raw(
    state: &AppState,
    credential_id: i64,
    label: &'static str,
    method: Method,
    path: &str,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<crate::credentials::control::RawControlResponse, PipelineError> {
    crate::credentials::control::execute_raw(
        state,
        credential_id,
        CredentialControlOperation::ClaudeRaw {
            label,
            method,
            path: path.to_owned(),
            query,
            headers,
            body,
        },
    )
    .await
    .map_err(|error| PipelineError::Transport(error.to_string()))
}

async fn forward_bound(
    state: &AppState,
    binding: &crate::store::persistence::records::CodexTaskBinding,
    label: &'static str,
    method: Method,
    path: &str,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<ExecOutcome, PipelineError> {
    Ok(raw_outcome(
        raw(
            state,
            binding.credential_id,
            label,
            method,
            path,
            query,
            headers,
            body,
        )
        .await?,
    ))
}

fn raw_outcome(response: crate::credentials::control::RawControlResponse) -> ExecOutcome {
    ExecOutcome {
        status: response.status,
        headers: response.headers,
        body: ResponseBody::Full(response.body),
        disposition: response.disposition,
    }
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
