//! Codex CLI services: the ChatGPT backend calls the CLI makes besides
//! Responses. Wire facts follow `samples/codex/codex-rs` (`backend-client`,
//! `core-plugins`, `codex-mcp`, `app-server-transport/remote_control`,
//! `codex-api/files`, `cloud-tasks`, `agent-identity`): plugins and MCP under
//! `/backend-api/ps/**`, account, settings, usage, tasks, environments and
//! remote control under `/backend-api/wham/**`, uploads under
//! `/backend-api/files/**`, workspace plugin sharing under
//! `/backend-api/public/plugins/**`, and the `GET
//! /v1/user-auth-credential/whoami` identity probe the CLI issues at
//! start-up. The CLI addresses the same endpoints two ways (`PathStyle` in
//! `backend-client`): `/backend-api/wham/**` on ChatGPT hosts and
//! `/api/codex/**` on Codex API hosts; both spellings (and the bare
//! `/codex/**`, `/ps/**` mounts) are normalized onto `/backend-api/**`
//! before classification.
//!
//! The route table is the policy. Each row's `ServiceClass` says what the
//! vendor endpoint returns; `call` renders it under the requested
//! `ServiceView`: Caller and Pool never reach the vendor for identity, usage,
//! settings or resource lists (they are synthesized from `ServiceCaller`
//! facts, resource items follow the binding's own credential), Credential
//! forwards raw with the named credential's auth. Catalog rows forward under
//! every view, key minting is refused under every view, and a path the table
//! does not know is 404 unless the view is Credential. Bodies and vendor
//! answers are otherwise passed through untouched, non-2xx included. Nothing
//! about the shared account is invented: synthetic ids hash the provider and
//! the caller identity (v3 `codex/surface/local.rs` shapes).

use super::common::invalid_config;
use super::headers::{account, backend_headers, base_urls, plan_type};
use super::{Codex, CodexConfig};
use crate::channel::{
    CallerUsage, CallerUsageWindow, ChannelError, ChannelServices, CredentialContext,
    CredentialView, HeaderAllowlist, OperationFuture, ResourceAccess, ServiceClass, ServiceContext,
    ServiceRoute, ServiceTransport,
};
use crate::channels::shared::services_common::{
    classify, create_bound, json_response, param, stable_id, unix_now_ms,
};
use gproxy_protocol::{HttpBody, WireRequest, WireResponse, capability::UpstreamConnection};
use http::{HeaderValue, Method, StatusCode, header};
use serde_json::{Map, Value, json};
use std::sync::OnceLock;

/// The identity probe answered locally.
const WHOAMI_PATH: &str = "/v1/user-auth-credential/whoami";
/// The remote-control socket; a WebSocket in `connect`, refused in `call`.
const REMOTE_CONTROL_PATH: &str = "/backend-api/wham/remote/control/server";

pub const KIND_TASK: &str = "codex:task";
pub const KIND_ENVIRONMENT: &str = "codex:environment";
pub const KIND_FILE: &str = "codex:file";
pub const KIND_PLUGIN: &str = "codex:plugin";
pub const KIND_REMOTE_SERVER: &str = "codex:remote_server";

const PRIMARY_WINDOW_MS: i64 = 5 * 60 * 60 * 1000;
const SECONDARY_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

use ResourceAccess::{Create, Delete, Item, List};
use ServiceClass::{
    Catalog, Identity, Prefix, Refused, Resource, Restricted, Settings, Telemetry, Usage,
};
use ServiceTransport::{Http, WebSocket};

const fn resource(kind: &'static str, access: ResourceAccess) -> ServiceClass {
    Resource { kind, access }
}

/// Known calls: method, canonical path template, transport, idempotent,
/// class. Specific rows precede the `/backend-api/{*path}` families.
const ROUTE_TABLE: &[(&str, &str, ServiceTransport, bool, ServiceClass)] = &[
    // Identity probe.
    ("GET", WHOAMI_PATH, Http, true, Identity),
    // Account, profile and settings (`backend-client/src/client.rs`).
    (
        "GET",
        "/backend-api/wham/accounts/check",
        Http,
        true,
        Identity,
    ),
    (
        "POST",
        "/backend-api/wham/accounts/send_add_credits_nudge_email",
        Http,
        false,
        Restricted,
    ),
    ("GET", "/backend-api/wham/profiles/me", Http, true, Identity),
    (
        "GET",
        "/backend-api/wham/settings/user",
        Http,
        true,
        Settings,
    ),
    (
        "GET",
        "/backend-api/wham/workspace-messages",
        Http,
        true,
        Settings,
    ),
    (
        "GET",
        "/backend-api/accounts/verified_access",
        Http,
        true,
        Identity,
    ),
    (
        "GET",
        "/backend-api/accounts/{account_id}/settings",
        Http,
        true,
        Identity,
    ),
    // Usage and rate-limit reset credits (`backend-client/src/client/*`).
    ("GET", "/backend-api/wham/usage", Http, true, Usage),
    (
        "POST",
        "/backend-api/wham/usage/thread_usage/query",
        Http,
        true,
        Usage,
    ),
    (
        "POST",
        "/backend-api/wham/usage/thread-estimates/query",
        Http,
        true,
        Usage,
    ),
    (
        "GET",
        "/backend-api/wham/rate-limit-reset-credits",
        Http,
        true,
        Usage,
    ),
    (
        "POST",
        "/backend-api/wham/rate-limit-reset-credits/consume",
        Http,
        false,
        Usage,
    ),
    (
        "POST",
        "/backend-api/analytics/codex/turn-costs",
        Http,
        false,
        Telemetry,
    ),
    // Cloud tasks and environments (`cloud-tasks`, `cloud-tasks-client`).
    (
        "GET",
        "/backend-api/wham/tasks",
        Http,
        true,
        resource(KIND_TASK, List),
    ),
    (
        "POST",
        "/backend-api/wham/tasks",
        Http,
        false,
        resource(KIND_TASK, Create),
    ),
    (
        "GET",
        "/backend-api/wham/tasks/list",
        Http,
        true,
        resource(KIND_TASK, List),
    ),
    (
        "GET",
        "/backend-api/wham/tasks/{task_id}",
        Http,
        true,
        resource(KIND_TASK, Item),
    ),
    (
        "GET",
        "/backend-api/wham/tasks/{task_id}/turns/{turn_id}/sibling_turns",
        Http,
        true,
        resource(KIND_TASK, Item),
    ),
    (
        "GET",
        "/backend-api/wham/environments",
        Http,
        true,
        resource(KIND_ENVIRONMENT, List),
    ),
    (
        "GET",
        "/backend-api/wham/environments/by-repo/{host}/{owner}/{repo}",
        Http,
        true,
        resource(KIND_ENVIRONMENT, List),
    ),
    // Configuration bundle, app updates, agent identity.
    (
        "GET",
        "/backend-api/wham/config/bundle",
        Http,
        true,
        Settings,
    ),
    ("GET", "/backend-api/wham/app/appcast", Http, true, Catalog),
    (
        "GET",
        "/backend-api/wham/agent-identities/jwks",
        Http,
        true,
        Catalog,
    ),
    (
        "POST",
        "/backend-api/v1/agent/register",
        Http,
        false,
        Restricted,
    ),
    (
        "POST",
        "/backend-api/v1/agent/{agent_runtime_id}/task/register",
        Http,
        false,
        Restricted,
    ),
    // Remote control (`app-server-transport/src/transport/remote_control`):
    // pairing binds devices to the real account.
    ("GET", REMOTE_CONTROL_PATH, WebSocket, true, Restricted),
    (
        "POST",
        "/backend-api/wham/remote/control/server/enroll",
        Http,
        false,
        resource(KIND_REMOTE_SERVER, Create),
    ),
    (
        "POST",
        "/backend-api/wham/remote/control/server/refresh",
        Http,
        false,
        Restricted,
    ),
    (
        "POST",
        "/backend-api/wham/remote/control/server/pair",
        Http,
        false,
        Restricted,
    ),
    (
        "GET",
        "/backend-api/wham/remote/control/server/pair/status",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/backend-api/wham/remote/control/environments/{environment_id}/clients",
        Http,
        true,
        resource(KIND_REMOTE_SERVER, List),
    ),
    (
        "DELETE",
        "/backend-api/wham/remote/control/environments/{environment_id}/clients/{client_id}",
        Http,
        false,
        resource(KIND_REMOTE_SERVER, Delete),
    ),
    // Codex API mount: models, guardian, alpha tools, analytics, realtime.
    ("GET", "/backend-api/codex/models", Http, true, Catalog),
    ("POST", "/backend-api/codex/guardian", Http, true, Catalog),
    (
        "POST",
        "/backend-api/codex/guardian-classifier",
        Http,
        true,
        Catalog,
    ),
    (
        "POST",
        "/backend-api/codex/alpha/{namespace}/v2/{tool}",
        Http,
        false,
        Restricted,
    ),
    (
        "POST",
        "/backend-api/codex/analytics-events/events",
        Http,
        false,
        Telemetry,
    ),
    (
        "POST",
        "/backend-api/codex/realtime/calls",
        Http,
        false,
        Catalog,
    ),
    // Uploads (`codex-api/src/files.rs`, `core/src/mcp_openai_file.rs`).
    (
        "POST",
        "/backend-api/files",
        Http,
        false,
        resource(KIND_FILE, Create),
    ),
    (
        "POST",
        "/backend-api/files/{file_id}/uploaded",
        Http,
        false,
        resource(KIND_FILE, Item),
    ),
    // Plugins and MCP (`core-plugins/src/remote*`, `codex-mcp`, `chatgpt`).
    ("GET", "/backend-api/ps/plugins/list", Http, true, Catalog),
    ("GET", "/backend-api/ps/plugins/search", Http, true, Catalog),
    (
        "GET",
        "/backend-api/ps/plugins/installed",
        Http,
        true,
        resource(KIND_PLUGIN, List),
    ),
    (
        "GET",
        "/backend-api/ps/plugins/suggested/codex",
        Http,
        true,
        Catalog,
    ),
    (
        "GET",
        "/backend-api/ps/plugins/workspace/created",
        Http,
        true,
        resource(KIND_PLUGIN, List),
    ),
    (
        "GET",
        "/backend-api/ps/plugins/workspace/shared",
        Http,
        true,
        resource(KIND_PLUGIN, List),
    ),
    (
        "GET",
        "/backend-api/ps/plugins/{plugin_id}",
        Http,
        true,
        Catalog,
    ),
    (
        "POST",
        "/backend-api/ps/plugins/{plugin_id}/install",
        Http,
        false,
        resource(KIND_PLUGIN, Create),
    ),
    (
        "POST",
        "/backend-api/ps/plugins/{plugin_id}/uninstall",
        Http,
        false,
        resource(KIND_PLUGIN, Delete),
    ),
    (
        "PUT",
        "/backend-api/ps/plugins/{plugin_id}/shares",
        Http,
        true,
        resource(KIND_PLUGIN, Item),
    ),
    ("POST", "/backend-api/ps/apps/batch", Http, true, Restricted),
    ("GET", "/backend-api/ps/mcp", Http, true, Restricted),
    ("POST", "/backend-api/ps/mcp", Http, false, Restricted),
    ("DELETE", "/backend-api/ps/mcp", Http, true, Restricted),
    ("GET", "/backend-api/plugins/featured", Http, true, Catalog),
    (
        "GET",
        "/backend-api/plugins/export/curated",
        Http,
        true,
        Catalog,
    ),
    (
        "GET",
        "/backend-api/connectors/directory/list",
        Http,
        true,
        Catalog,
    ),
    (
        "GET",
        "/backend-api/connectors/directory/list_workspace",
        Http,
        true,
        Restricted,
    ),
    // Workspace plugin publishing (`core-plugins/src/remote/share.rs`).
    (
        "POST",
        "/backend-api/public/plugins/workspace",
        Http,
        false,
        Restricted,
    ),
    (
        "POST",
        "/backend-api/public/plugins/workspace/upload-url",
        Http,
        false,
        Restricted,
    ),
    (
        "POST",
        "/backend-api/public/plugins/workspace/{plugin_id}",
        Http,
        false,
        Restricted,
    ),
    (
        "DELETE",
        "/backend-api/public/plugins/workspace/{plugin_id}",
        Http,
        false,
        Restricted,
    ),
    // Prefix families: anything else under the vendor prefix.
    ("GET", "/backend-api/{*path}", Http, true, Prefix),
    ("POST", "/backend-api/{*path}", Http, false, Prefix),
    ("PUT", "/backend-api/{*path}", Http, false, Prefix),
    ("PATCH", "/backend-api/{*path}", Http, false, Prefix),
    ("DELETE", "/backend-api/{*path}", Http, false, Prefix),
];

fn routes() -> &'static [ServiceRoute] {
    static ROUTES: OnceLock<Vec<ServiceRoute>> = OnceLock::new();
    ROUTES.get_or_init(|| {
        ROUTE_TABLE
            .iter()
            .map(
                |(method, path_template, transport, idempotent, class)| ServiceRoute {
                    method: Method::from_bytes(method.as_bytes()).expect("a standard method"),
                    path_template,
                    transport: *transport,
                    idempotent: *idempotent,
                    class: *class,
                },
            )
            .collect()
    })
}

/// The canonical backend path for a client path: `/backend-api/**`
/// verbatim, the Codex-API spelling `/api/codex/**` onto
/// `/backend-api/wham/**`, the bare `/codex/**` and `/ps/**` families onto
/// their backend mounts, and the whoami probe as is.
fn canonical_path(path: &str) -> Option<String> {
    if path == WHOAMI_PATH || path == "/backend-api" || path.starts_with("/backend-api/") {
        return Some(path.to_owned());
    }
    for (alias, mount) in [
        ("/api/codex", "/backend-api/wham"),
        ("/codex", "/backend-api/codex"),
        ("/ps", "/backend-api/ps"),
    ] {
        if path == alias {
            return Some(mount.to_owned());
        }
        if let Some(rest) = path
            .strip_prefix(alias)
            .filter(|rest| rest.starts_with('/'))
        {
            return Some(format!("{mount}{rest}"));
        }
    }
    None
}

/// The client query minus any `key` credential parameter.
fn forwarded_query(source: Option<String>) -> Option<String> {
    let source = source.unwrap_or_default();
    let kept = source
        .split('&')
        .filter(|pair| !pair.is_empty() && pair.split('=').next() != Some("key"))
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

/// A backend URL for a canonical path, its scheme flipped for a WebSocket
/// handshake. `backend` is the `/backend-api` root `base_urls` derives (the
/// CLI's own `chatgpt_base_url`), so the prefix is joined relative to it
/// rather than repeated; a mirror that mounts the backend elsewhere keeps
/// working.
fn backend_url(backend: &str, path: &str, query: Option<String>, websocket: bool) -> String {
    let relative = path.strip_prefix("/backend-api").unwrap_or(path);
    let mut url = format!("{backend}{relative}");
    if websocket {
        url = url
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
    }
    match query {
        Some(query) => format!("{url}?{query}"),
        None => url,
    }
}

/// The forwarded request: backend URL, the credential's own headers with the
/// client's forwardable ones, the body untouched.
fn forward<B>(
    account: &CredentialContext<'_>,
    request: WireRequest<B>,
    websocket: bool,
) -> Result<http::Request<B>, ChannelError> {
    let config = CodexConfig::from_view(account.provider)?;
    let identity = super::headers::account(&account.credential)?;
    let (_, backend) = base_urls(account.provider);
    let path = canonical_path(&request.path).ok_or(ChannelError::UnsupportedService)?;
    let allowlist = HeaderAllowlist::from_view_for(account.provider, super::CLI_HEADERS)?;
    let headers = backend_headers(
        &config,
        &identity,
        Some((&request.headers, allowlist.as_ref())),
    )?;
    let uri = backend_url(&backend, &path, forwarded_query(request.query), websocket);
    let mut builder = http::Request::builder().method(request.method).uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    builder
        .body(request.body)
        .map_err(|error| invalid_config(error.to_string()))
}

async fn send<'a>(
    account: &CredentialContext<'a>,
    request: WireRequest<HttpBody>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    Ok(account
        .client
        .send(forward(account, request, false)?)
        .await?)
}

// --------------------------------------------------------------- answers

/// The ChatGPT backend's error envelope.
fn error_response(status: StatusCode, detail: &str) -> WireResponse<HttpBody> {
    json_response(status, &json!({"detail": detail}))
}

fn not_found() -> WireResponse<HttpBody> {
    error_response(StatusCode::NOT_FOUND, "Not found")
}

fn forbidden(detail: &str) -> WireResponse<HttpBody> {
    error_response(StatusCode::FORBIDDEN, detail)
}

/// A public account fact: host metadata first, then the secret's
/// `provider_fields` the login recorded.
fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a Value> {
    credential
        .metadata
        .get(name)
        .or_else(|| {
            credential
                .secret
                .pointer(&format!("/provider_fields/{name}"))
        })
        .filter(|value| !value.is_null())
}

/// `whoami` under the Credential view: the account's own facts as the login
/// recorded them, null where unknown. The CLI asks this of its auth API
/// host, not the ChatGPT backend, so it is never forwarded.
fn whoami_from_facts(credential: &CredentialView<'_>) -> Value {
    let account_id = account(credential)
        .ok()
        .and_then(|a| a.account_id)
        .map(Value::String)
        .unwrap_or(Value::Null);
    json!({
        "email": fact(credential, "email").cloned().unwrap_or(Value::Null),
        "chatgpt_user_id": fact(credential, "chatgpt_user_id").cloned().unwrap_or(Value::Null),
        "chatgpt_account_id": account_id,
        "chatgpt_plan_type": plan_type(credential).map(Value::String).unwrap_or(Value::Null),
        "chatgpt_account_is_fedramp": fact(credential, "chatgpt_account_is_fedramp")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// The plan tier is not identifying; the credential's own, else `pro`.
fn plan(credential: &CredentialView<'_>) -> String {
    plan_type(credential).unwrap_or_else(|| "pro".to_owned())
}

/// Synthetic account facts for a synthesized view: stable ids from the
/// provider and the caller (or pool) identity, never the credential's.
struct Synthetic {
    name: String,
    user_id: String,
    account_id: String,
}

fn synthetic(ctx: &ServiceContext<'_>) -> Synthetic {
    let identity = ctx.caller.identity();
    let provider = ctx.account.provider.id;
    Synthetic {
        name: identity
            .display_name
            .clone()
            .unwrap_or_else(|| identity.id.clone()),
        user_id: stable_id("codex", "user", provider, &identity.id),
        account_id: stable_id("codex", "account", provider, &identity.id),
    }
}

fn identity_answer(path: &str, ctx: &ServiceContext<'_>) -> Value {
    let me = synthetic(ctx);
    let plan = plan(&ctx.account.credential);
    match path {
        WHOAMI_PATH => json!({
            "email": format!("{}@gproxy.invalid", me.name),
            "chatgpt_user_id": me.user_id,
            "chatgpt_account_id": me.account_id,
            "chatgpt_plan_type": plan,
            "chatgpt_account_is_fedramp": false,
        }),
        "/backend-api/wham/accounts/check" => json!({
            "accounts": [{
                "id": me.account_id,
                "name": me.name,
                "profile_picture_url": null,
                "structure": plan,
            }],
            "account_ordering": [me.account_id],
            "default_account_id": me.account_id,
        }),
        "/backend-api/accounts/verified_access" => json!({"programs": []}),
        _ => {
            // `/backend-api/accounts/{account_id}/settings`: workspace beta
            // flags, from provider config as in v3.
            let mut beta_settings = Map::new();
            if let Some(enabled) = ctx
                .account
                .provider
                .config
                .get("codex_plugins_enabled")
                .and_then(Value::as_bool)
            {
                beta_settings.insert("enable_plugins".into(), Value::Bool(enabled));
            }
            json!({"beta_settings": beta_settings})
        }
    }
}

/// `profiles/me` needs the usage totals as well as the identity.
fn profile_answer(usage: &CallerUsage) -> Value {
    json!({
        "stats": {
            "lifetime_tokens": usage.input_tokens.saturating_add(usage.output_tokens),
            "peak_daily_tokens": null,
            "longest_running_turn_sec": null,
            "current_streak_days": null,
            "longest_streak_days": null,
            "daily_usage_buckets": null,
        }
    })
}

fn select_window<'w>(
    windows: &'w [CallerUsageWindow],
    keys: &[&str],
    duration_ms: i64,
) -> Option<&'w CallerUsageWindow> {
    windows
        .iter()
        .filter(|window| {
            keys.iter().any(|key| window.key.eq_ignore_ascii_case(key))
                || window
                    .period_start_ms
                    .zip(window.reset_at_ms)
                    .is_some_and(|(start, reset)| reset.saturating_sub(start) == duration_ms)
        })
        .max_by(|a, b| {
            a.used_percent
                .unwrap_or(0.0)
                .total_cmp(&b.used_percent.unwrap_or(0.0))
        })
}

fn window_payload(window: &CallerUsageWindow, now_ms: i64) -> Value {
    let mut payload = Map::new();
    if let Some(percent) = window.used_percent {
        payload.insert(
            "used_percent".into(),
            Value::from(percent.clamp(0.0, 100.0).round() as i64),
        );
    }
    if let Some(reset) = window.reset_at_ms {
        payload.insert("reset_at".into(), Value::from(reset / 1000));
        payload.insert(
            "reset_after_seconds".into(),
            Value::from((reset.saturating_sub(now_ms) / 1000).max(0)),
        );
        if let Some(start) = window.period_start_ms {
            payload.insert(
                "limit_window_seconds".into(),
                Value::from(reset.saturating_sub(start) / 1000),
            );
        }
    }
    Value::Object(payload)
}

/// `GET wham/usage` in the shape the CLI parses (v3 `codex/surface/usage.rs`):
/// the caller's windows as primary/secondary, no window fields when the host
/// allots none, credits always empty.
fn usage_answer(usage: &CallerUsage, credential: &CredentialView<'_>) -> Value {
    let now = unix_now_ms();
    let primary = select_window(
        &usage.windows,
        &["primary", "5h", "five_hour", "five-hour"],
        PRIMARY_WINDOW_MS,
    );
    let secondary = select_window(
        &usage.windows,
        &["secondary", "7d", "seven_day", "seven-day"],
        SECONDARY_WINDOW_MS,
    );
    let reached = primary
        .iter()
        .chain(secondary.iter())
        .any(|w| w.used_percent.is_some_and(|p| p >= 100.0));
    let complete = primary.is_some_and(|w| w.used_percent.is_some())
        && secondary.is_some_and(|w| w.used_percent.is_some());
    let mut rate_limit = Map::new();
    if let Some(window) = primary {
        rate_limit.insert("primary_window".into(), window_payload(window, now));
    }
    if let Some(window) = secondary {
        rate_limit.insert("secondary_window".into(), window_payload(window, now));
    }
    if reached || complete {
        rate_limit.insert("allowed".into(), Value::Bool(!reached));
        rate_limit.insert("limit_reached".into(), Value::Bool(reached));
    }
    let mut response = Map::from_iter([
        ("plan_type".to_owned(), Value::String(plan(credential))),
        (
            "rate_limit_reset_credits".to_owned(),
            json!({"available_count": 0}),
        ),
        (
            "local_usage".to_owned(),
            json!({
                "cost": usage.cost,
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
            }),
        ),
    ]);
    if !rate_limit.is_empty() {
        response.insert("rate_limit".into(), Value::Object(rate_limit));
    }
    if reached {
        response.insert(
            "rate_limit_reached_type".into(),
            json!({"type": "rate_limit_reached"}),
        );
    }
    Value::Object(response)
}

fn usage_route_answer(path: &str, usage: &CallerUsage, credential: &CredentialView<'_>) -> Value {
    match path {
        "/backend-api/wham/usage" => usage_answer(usage, credential),
        // The host meters callers, not threads: no fabricated groups.
        "/backend-api/wham/usage/thread_usage/query"
        | "/backend-api/wham/usage/thread-estimates/query" => json!({"threads": []}),
        "/backend-api/wham/rate-limit-reset-credits" => {
            json!({"available_count": 0, "credits": []})
        }
        _ => json!({"code": "no_credit", "windows_reset": 0}),
    }
}

/// Neutral defaults, overridable from provider config (v3 key names).
fn settings_answer(path: &str, config: &Value) -> Value {
    match path {
        "/backend-api/wham/settings/user" => config
            .get("codex_virtual_settings")
            .cloned()
            .unwrap_or_else(|| json!({"commit_attribution_enabled": false})),
        "/backend-api/wham/workspace-messages" => json!({
            "messages": config
                .get("codex_workspace_messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        }),
        _ => config
            .get("codex_config_bundle")
            .cloned()
            .unwrap_or_else(|| json!({"config_toml": null, "requirements_toml": null})),
    }
}

// ------------------------------------------------------------- resources

fn list_answer(kind: &str, summaries: Vec<Value>) -> Value {
    match kind {
        KIND_TASK => json!({"tasks": summaries, "cursor": null}),
        KIND_ENVIRONMENT => Value::Array(summaries),
        KIND_PLUGIN => json!({"plugins": summaries, "pagination": {"next_page_token": null}}),
        _ => json!({"clients": summaries}),
    }
}

fn created_id(kind: &str) -> fn(&Value) -> Option<String> {
    match kind {
        KIND_TASK => |v| {
            v.pointer("/task/id")
                .or_else(|| v.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        },
        KIND_FILE => |v| v.get("file_id").and_then(Value::as_str).map(str::to_owned),
        KIND_REMOTE_SERVER => |v| {
            v.get("server_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        },
        _ => |v| v.get("id").and_then(Value::as_str).map(str::to_owned),
    }
}

/// The path parameter naming the resource an item route acts on.
fn item_param(kind: &str) -> &'static str {
    match kind {
        KIND_TASK => "task_id",
        KIND_FILE => "file_id",
        KIND_PLUGIN => "plugin_id",
        _ => "client_id",
    }
}

async fn resource_call<'a>(
    kind: &'static str,
    access: ResourceAccess,
    params: &[(&'static str, &str)],
    ctx: ServiceContext<'a>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    match access {
        List => {
            let summaries = ctx
                .caller
                .list_bindings(kind)
                .await?
                .into_iter()
                .map(|record| record.summary)
                .collect();
            Ok(json_response(StatusCode::OK, &list_answer(kind, summaries)))
        }
        Create => {
            let fallback = param(params, item_param(kind)).map(str::to_owned);
            let account = ctx.account;
            let upstream = forward(&account, ctx.request, false)?;
            create_bound(
                ctx.caller,
                kind,
                &account,
                upstream,
                created_id(kind),
                fallback.as_deref(),
            )
            .await
        }
        Item | Delete => {
            let Some(id) = param(params, item_param(kind)) else {
                return Ok(not_found());
            };
            let Some(binding) = ctx.caller.find_binding(kind, id).await? else {
                return Ok(not_found());
            };
            let Some(account) = ctx.account_for(&binding.credential_id) else {
                return Ok(not_found());
            };
            let response = send(&account, ctx.request).await?;
            if access == Delete && response.status.is_success() {
                ctx.caller.delete_binding(kind, id).await?;
            }
            Ok(response)
        }
    }
}

// ------------------------------------------------------------- dispatch

impl ChannelServices for Codex {
    fn routes(&self) -> &[ServiceRoute] {
        routes()
    }

    fn call<'a>(&'a self, ctx: ServiceContext<'a>) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            let path = canonical_path(&ctx.request.path).ok_or(ChannelError::UnsupportedService)?;
            let Some((route, params)) = classify(routes(), &ctx.request.method, &path) else {
                return Err(ChannelError::UnsupportedService);
            };
            let params = params
                .into_iter()
                .map(|(k, v)| (k, v.to_owned()))
                .collect::<Vec<_>>();
            let params = params
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect::<Vec<_>>();
            let raw = !ctx.view.is_synthesized();
            // The remote-control socket has no HTTP answer under any view.
            if route.transport == WebSocket {
                return Ok(if raw {
                    let mut response = error_response(
                        StatusCode::UPGRADE_REQUIRED,
                        "the remote-control server endpoint is a WebSocket; send an Upgrade request",
                    );
                    response
                        .headers
                        .insert(header::UPGRADE, HeaderValue::from_static("websocket"));
                    response
                } else {
                    forbidden("remote control pairs devices with the shared account")
                });
            }
            match route.class {
                Refused => Ok(forbidden(
                    "this operation is not allowed through the gateway",
                )),
                Catalog => send(&ctx.account, ctx.request).await,
                Identity if raw && path == WHOAMI_PATH => Ok(json_response(
                    StatusCode::OK,
                    &whoami_from_facts(&ctx.account.credential),
                )),
                _ if raw => send(&ctx.account, ctx.request).await,
                Telemetry => Ok(json_response(StatusCode::OK, &json!({}))),
                Restricted => Ok(forbidden(
                    "this operation acts on the shared account and is not available in this view",
                )),
                Prefix => Ok(not_found()),
                Identity if path == "/backend-api/wham/profiles/me" => {
                    let usage = ctx.caller.usage().await?;
                    Ok(json_response(StatusCode::OK, &profile_answer(&usage)))
                }
                Identity => Ok(json_response(StatusCode::OK, &identity_answer(&path, &ctx))),
                Usage => {
                    let usage = ctx.caller.usage().await?;
                    Ok(json_response(
                        StatusCode::OK,
                        &usage_route_answer(&path, &usage, &ctx.account.credential),
                    ))
                }
                Settings => Ok(json_response(
                    StatusCode::OK,
                    &settings_answer(&path, ctx.account.provider.config),
                )),
                Resource { kind, access } => resource_call(kind, access, &params, ctx).await,
            }
        })
    }

    fn connect<'a>(
        &'a self,
        ctx: ServiceContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        Box::pin(async move {
            if ctx.view.is_synthesized() {
                return Ok(UpstreamConnection::Rejected(forbidden(
                    "remote control pairs devices with the shared account",
                )));
            }
            let upstream = forward(&ctx.account, ctx.request, true)?;
            Ok(ctx.account.client.connect(upstream).await?)
        })
    }
}

/// Expose the classification for tests and hosts.
pub fn service_routes() -> &'static [ServiceRoute] {
    routes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_paths_map_onto_the_backend() {
        assert_eq!(
            canonical_path("/backend-api/ps/plugins/list").as_deref(),
            Some("/backend-api/ps/plugins/list")
        );
        assert_eq!(
            canonical_path("/api/codex/settings/user").as_deref(),
            Some("/backend-api/wham/settings/user")
        );
        assert_eq!(
            canonical_path("/codex/models").as_deref(),
            Some("/backend-api/codex/models")
        );
        assert_eq!(
            canonical_path("/ps/mcp").as_deref(),
            Some("/backend-api/ps/mcp")
        );
        assert_eq!(canonical_path("/psx/mcp"), None);
        assert_eq!(canonical_path("/v1/responses"), None);
    }

    #[test]
    fn the_key_query_parameter_is_stripped() {
        assert_eq!(
            forwarded_query(Some("a=1&key=secret&b=2".into())).as_deref(),
            Some("a=1&b=2")
        );
        assert_eq!(forwarded_query(Some("key=secret".into())), None);
        assert_eq!(forwarded_query(None), None);
    }

    #[test]
    fn backend_paths_are_joined_relative_to_the_backend_root() {
        assert_eq!(
            backend_url(
                "https://chatgpt.com/backend-api",
                "/backend-api/wham/usage",
                None,
                false
            ),
            "https://chatgpt.com/backend-api/wham/usage"
        );
        assert_eq!(
            backend_url(
                "https://mirror.example/proxy",
                "/backend-api/wham/remote/control/server",
                Some("a=1".into()),
                true
            ),
            "wss://mirror.example/proxy/wham/remote/control/server?a=1",
        );
    }

    #[test]
    fn every_listed_route_is_classified_and_canonical() {
        for route in routes() {
            assert!(
                canonical_path(route.path_template).is_some(),
                "{} is outside the vendor prefix",
                route.path_template
            );
            let (matched, _) = classify(
                routes(),
                &route.method,
                route.path_template.replace("{*path}", "x/y").as_str(),
            )
            .expect("declared route matches itself");
            assert_eq!(matched.class, route.class, "{}", route.path_template);
        }
        // Specific rows win over the prefix family.
        let (route, _) = classify(routes(), &Method::GET, "/backend-api/wham/usage").unwrap();
        assert_eq!(route.class, Usage);
        let (route, _) = classify(routes(), &Method::GET, "/backend-api/wham/new-thing").unwrap();
        assert_eq!(route.class, Prefix);
    }
}
