//! Claude Code CLI services: the OAuth-scoped `/api/**` calls the CLI makes
//! besides Messages. Wire facts follow `samples/claude-code-2.1.252`
//! (`CLAUDE_AI_OAUTH_ROUTES.md`, `ANALYSIS.md` and the route literals in the
//! native binary): profile, validate, roles, bootstrap, usage, policy
//! limits, account settings, file upload/download, organization connectors,
//! plugins and skills, onboarding and billing controls, desktop update
//! redirects and the Claude Design (`frame`) surface. `ANALYSIS.md` records
//! that all of them resolve against `BASE_API_URL` (`api.anthropic.com`);
//! `claude.ai` only serves the cookie login, so `claude_ai_url` is not
//! consulted here.
//!
//! The route table is the policy. Each row's `ServiceClass` says what the
//! vendor endpoint returns; `call` renders it under the requested
//! `ServiceView`: Caller and Pool never reach the vendor for identity, usage,
//! settings or resource lists (synthesized from `ServiceCaller` facts,
//! resource items follow the binding's own credential), Credential forwards
//! raw with the named credential's bearer, the `oauth-2025-04-20` beta and
//! the CLI identity headers. Catalog rows forward under every view, key
//! minting (`create_api_key`, a long-lived key on the shared subscription)
//! is refused under every view, and a path the table does not know is 404
//! unless the view is Credential. Synthetic ids hash the provider and the
//! caller identity (v3 `claudecode/surface/local.rs` shapes).

use super::{
    ANTHROPIC_VERSION, CHANNEL_HEADERS, CLI_USER_AGENT, Claudecode, ClaudecodeConfig, OAUTH_BETA,
    STATIC_HEADERS, base_url, fact, header_value, invalid_config, merge_beta, stainless_arch,
    stainless_os, valid_cli_user_agent,
};
use crate::channel::{
    CallerRole, CallerUsage, CallerUsageWindow, ChannelError, ChannelServices, CredentialContext,
    CredentialView, HeaderAllowlist, OperationFuture, ResourceAccess, ServiceClass, ServiceContext,
    ServiceRoute, ServiceTransport,
};
use crate::channels::shared::services_common::{
    classify, create_bound, json_of, json_response, keyword_filter, keywords, param, read_body,
    stable_id,
};
use gproxy_protocol::{HttpBody, WireRequest, WireResponse};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};
use std::sync::OnceLock;

/// The vendor prefix every CLI control call lives under.
const VENDOR_PREFIX: &str = "/api/";

pub const KIND_FILE: &str = "claude:file";
pub const KIND_SKILL: &str = "claude:skill";
pub const KIND_PLUGIN: &str = "claude:plugin";

const FIVE_HOURS_MS: i64 = 5 * 60 * 60 * 1000;
const SEVEN_DAYS_MS: i64 = 7 * 24 * 60 * 60 * 1000;

use ResourceAccess::{Create, Item, List};
use ServiceClass::{
    Catalog, Identity, Prefix, Refused, Resource, Restricted, Settings, Telemetry, Usage,
};
use ServiceTransport::Http;

const fn resource(kind: &'static str, access: ResourceAccess) -> ServiceClass {
    Resource { kind, access }
}

/// Known calls: method, vendor path template, transport, idempotent, class.
/// Specific rows precede the `/api/{*path}` families.
const ROUTE_TABLE: &[(&str, &str, ServiceTransport, bool, ServiceClass)] = &[
    // Identity, validation and roles.
    ("GET", "/api/hello", Http, true, Catalog),
    ("GET", "/api/hello/{name}", Http, true, Catalog),
    ("GET", "/api/oauth/profile", Http, true, Identity),
    ("POST", "/api/oauth/validate", Http, true, Identity),
    ("GET", "/api/oauth/claude_cli/roles", Http, true, Identity),
    (
        "POST",
        "/api/oauth/claude_cli/create_api_key",
        Http,
        false,
        Refused,
    ),
    ("GET", "/api/oauth/cri", Http, true, Identity),
    ("GET", "/api/claude_cli_profile", Http, true, Identity),
    ("POST", "/api/auth/trusted_devices", Http, false, Restricted),
    // Bootstrap, usage, policy and organization state.
    ("GET", "/api/claude_cli/bootstrap", Http, true, Identity),
    ("GET", "/api/oauth/usage", Http, true, Usage),
    (
        "GET",
        "/api/claude_code/policy_limits",
        Http,
        true,
        Settings,
    ),
    ("GET", "/api/claude_code/settings", Http, true, Settings),
    ("GET", "/api/claude_code/skills", Http, true, Settings),
    (
        "GET",
        "/api/claude_code/notification/preferences",
        Http,
        true,
        Settings,
    ),
    (
        "GET",
        "/api/claude_code/organizations/metrics_enabled",
        Http,
        true,
        Settings,
    ),
    ("POST", "/api/claude_code/metrics", Http, false, Telemetry),
    (
        "GET",
        "/api/organization/claude_code_first_token_date",
        Http,
        true,
        Settings,
    ),
    ("GET", "/api/claude_code_penguin_mode", Http, true, Settings),
    ("GET", "/api/claude_code_grove", Http, true, Settings),
    ("GET", "/api/oauth/account/settings", Http, true, Settings),
    ("PATCH", "/api/oauth/account/settings", Http, true, Settings),
    (
        "POST",
        "/api/oauth/account/grove_notice_viewed",
        Http,
        true,
        Settings,
    ),
    ("POST", "/api/claude_cli_feedback", Http, false, Telemetry),
    (
        "POST",
        "/api/claude_code_shared_session_transcripts",
        Http,
        false,
        Telemetry,
    ),
    // Attachments.
    (
        "POST",
        "/api/oauth/file_upload",
        Http,
        false,
        resource(KIND_FILE, Create),
    ),
    (
        "GET",
        "/api/oauth/files/{file_uuid}/content",
        Http,
        true,
        resource(KIND_FILE, Item),
    ),
    // Organization catalogs and connectors (`x-organization-uuid`).
    (
        "POST",
        "/api/oauth/organizations/{org}/mcp/connectors/search",
        Http,
        true,
        Settings,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/mcp/connectors/suggest",
        Http,
        true,
        Settings,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/mcp/connectors/list",
        Http,
        true,
        Settings,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/plugins/search",
        Http,
        true,
        resource(KIND_PLUGIN, List),
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/plugins/list-plugins",
        Http,
        true,
        resource(KIND_PLUGIN, List),
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/plugins/{plugin_id}/download",
        Http,
        true,
        resource(KIND_PLUGIN, Item),
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/skills/search",
        Http,
        true,
        resource(KIND_SKILL, List),
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/skills/list-skills",
        Http,
        true,
        resource(KIND_SKILL, List),
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/skills/{skill_id}/download",
        Http,
        true,
        resource(KIND_SKILL, Item),
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/plugin_ratings",
        Http,
        false,
        Telemetry,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/plugin_ratings/appearances",
        Http,
        false,
        Telemetry,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/plugin_ratings/appearances/outcome",
        Http,
        false,
        Telemetry,
    ),
    (
        "DELETE",
        "/api/oauth/organizations/{org}/plugin_ratings",
        Http,
        true,
        Telemetry,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/sync/github/auth",
        Http,
        true,
        Restricted,
    ),
    // Organization billing, trials and admin requests.
    (
        "GET",
        "/api/oauth/organizations/{org}/payment_method",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/prepaid/bundles",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/prepaid/credits",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/prepaid/commits/{commit_id}",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/admin_requests/eligibility",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/admin_requests/me",
        Http,
        true,
        Restricted,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/admin_requests",
        Http,
        false,
        Restricted,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/billing/tax_rate",
        Http,
        true,
        Restricted,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/claude_code/pro_trial",
        Http,
        false,
        Restricted,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/contracts/prepaid/credits",
        Http,
        false,
        Restricted,
    ),
    (
        "PUT",
        "/api/oauth/organizations/{org}/contracts/auto_reload_settings",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/overage_credit_grant",
        Http,
        true,
        Restricted,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/overage_credit_grant",
        Http,
        false,
        Restricted,
    ),
    (
        "PUT",
        "/api/oauth/organizations/{org}/overage_spend_limit",
        Http,
        true,
        Restricted,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/setup_overage_billing",
        Http,
        false,
        Restricted,
    ),
    (
        "GET",
        "/api/oauth/organizations/{org}/referral/eligibility",
        Http,
        true,
        Restricted,
    ),
    (
        "POST",
        "/api/oauth/organizations/{org}/referral/redemptions",
        Http,
        false,
        Restricted,
    ),
    // Organization resources outside the OAuth mount.
    (
        "POST",
        "/api/organizations/{org}/reset_rate_limits",
        Http,
        false,
        Restricted,
    ),
    (
        "GET",
        "/api/organizations/{org}/model_selector/{selector}",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/organizations/{org}/claude_code/onboarding",
        Http,
        true,
        Restricted,
    ),
    (
        "POST",
        "/api/organizations/{org}/claude_code/onboarding",
        Http,
        false,
        Restricted,
    ),
    (
        "GET",
        "/api/organizations/{org}/cowork/remote_devices",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/organizations/{org}/marketplace/plugin",
        Http,
        true,
        Restricted,
    ),
    (
        "GET",
        "/api/organizations/{org}/projects/{*path}",
        Http,
        true,
        Restricted,
    ),
    // Desktop update redirects (public) and the Claude Design surface
    // (account-scoped frames).
    ("GET", "/api/desktop/{*path}", Http, true, Catalog),
    ("GET", "/api/frame/{*path}", Http, true, Restricted),
    ("POST", "/api/frame/{*path}", Http, false, Restricted),
    // Prefix families: anything else under the vendor prefix.
    ("GET", "/api/{*path}", Http, true, Prefix),
    ("POST", "/api/{*path}", Http, false, Prefix),
    ("PUT", "/api/{*path}", Http, false, Prefix),
    ("PATCH", "/api/{*path}", Http, false, Prefix),
    ("DELETE", "/api/{*path}", Http, false, Prefix),
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

/// Expose the classification for tests and hosts.
pub fn service_routes() -> &'static [ServiceRoute] {
    routes()
}

fn is_vendor_path(path: &str) -> bool {
    path.len() > VENDOR_PREFIX.len() && path.starts_with(VENDOR_PREFIX)
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

/// Headers every forwarded service call carries: the client's forwardable
/// ones (never its authorization, cookie or the channel's identity set),
/// then the bearer, the OAuth beta merged ahead of the client's betas, the
/// API version, the CLI identity and static config headers. Unlike Messages
/// the content type is the client's: uploads are multipart.
fn service_headers(
    config: &ClaudecodeConfig,
    access_token: &str,
    source: &HeaderMap,
    allowlist: Option<&HeaderAllowlist>,
) -> Result<HeaderMap, ChannelError> {
    let client_header = |name: &'static str| {
        let name = HeaderName::from_static(name);
        if allowlist.is_some_and(|list| !list.allows(&name)) {
            return None;
        }
        source.get(&name).cloned()
    };
    let client_user_agent = client_header("user-agent");
    let client_beta = client_header("anthropic-beta");
    let client_accept = client_header("accept");
    let mut headers = forwardable(source, allowlist, CHANNEL_HEADERS);
    if let Some(beta) = client_beta {
        headers.insert(HeaderName::from_static("anthropic-beta"), beta);
    }
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {access_token}"))?,
    );
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    let beta = merge_beta(&headers);
    debug_assert!(beta.starts_with(OAUTH_BETA));
    headers.insert(
        HeaderName::from_static("anthropic-beta"),
        header_value(&beta)?,
    );
    for (name, value) in STATIC_HEADERS {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    headers.insert(
        HeaderName::from_static("x-stainless-os"),
        HeaderValue::from_static(stainless_os()),
    );
    headers.insert(
        HeaderName::from_static("x-stainless-arch"),
        HeaderValue::from_static(stainless_arch()),
    );
    let user_agent = client_user_agent
        .filter(|value| value.to_str().is_ok_and(valid_cli_user_agent))
        .unwrap_or_else(|| HeaderValue::from_static(CLI_USER_AGENT));
    headers.insert(header::USER_AGENT, user_agent);
    headers.insert(
        header::ACCEPT,
        client_accept
            .unwrap_or_else(|| HeaderValue::from_static("application/json, text/plain, */*")),
    );
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| invalid_config(format!("header `{name}`")))?,
            HeaderValue::from_str(value).map_err(|_| invalid_config(format!("header `{name}`")))?,
        );
    }
    Ok(headers)
}

use crate::channel::forwardable;

fn forward(
    account: &CredentialContext<'_>,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let config = ClaudecodeConfig::from_view(account.provider)?;
    let identity = super::account(&account.credential)?;
    let allowlist = HeaderAllowlist::from_view_for(
        account.provider,
        crate::channel::ChannelHeaders {
            names: &[
                "anthropic-beta",
                "x-claude-code-session-id",
                "session_id",
                "user-agent",
                "accept",
                "cache-control",
                "x-organization-uuid",
            ],
            prefixes: &[],
        },
    )?;
    let headers = service_headers(
        &config,
        identity.access_token,
        &request.headers,
        allowlist.as_ref(),
    )?;
    let url = format!("{}{}", base_url(account.provider), request.path);
    let uri = match forwarded_query(request.query) {
        Some(query) => format!("{url}?{query}"),
        None => url,
    };
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
    Ok(account.client.send(forward(account, request)?).await?)
}

// --------------------------------------------------------------- answers

/// The Anthropic error envelope, as the CLI expects to parse it.
fn error_response(status: StatusCode, kind: &str, message: &str) -> WireResponse<HttpBody> {
    json_response(
        status,
        &json!({"type": "error", "error": {"type": kind, "message": message}}),
    )
}

fn not_found(message: &str) -> WireResponse<HttpBody> {
    error_response(StatusCode::NOT_FOUND, "not_found_error", message)
}

fn forbidden(message: &str) -> WireResponse<HttpBody> {
    error_response(StatusCode::FORBIDDEN, "permission_error", message)
}

/// The rate-limit tier is not identifying; the credential's own, else the
/// gateway's placeholder.
fn tier(credential: &CredentialView<'_>) -> String {
    fact(credential, "rate_limit_tier")
        .or_else(|| fact(credential, "organization_type"))
        .unwrap_or("gproxy")
        .to_owned()
}

struct Synthetic {
    name: String,
    account_uuid: String,
    organization_uuid: String,
}

fn synthetic(ctx: &ServiceContext<'_>) -> Synthetic {
    let identity = ctx.caller.identity();
    let provider = ctx.account.provider.id;
    Synthetic {
        name: identity
            .display_name
            .clone()
            .unwrap_or_else(|| identity.id.clone()),
        account_uuid: stable_id("claude", "account", provider, &identity.id),
        organization_uuid: stable_id("claude", "organization", provider, &identity.id),
    }
}

fn identity_answer(path: &str, ctx: &ServiceContext<'_>) -> Value {
    let me = synthetic(ctx);
    let provider = ctx.account.provider;
    match path {
        "/api/oauth/profile" | "/api/claude_cli_profile" => json!({
            "account": {
                "uuid": me.account_uuid,
                "email": format!("{}@gproxy.invalid", me.name),
                "display_name": me.name,
            },
            "organization": {
                "uuid": me.organization_uuid,
                "name": provider.id,
                "organization_type": "api",
                "rate_limit_tier": tier(&ctx.account.credential),
            }
        }),
        "/api/oauth/claude_cli/roles" => {
            let role = match ctx.caller.role() {
                CallerRole::Admin => "admin",
                CallerRole::Member => "member",
            };
            json!({
                "organization_role": role,
                "workspace_role": role,
                "organization_name": provider.id,
            })
        }
        "/api/claude_cli/bootstrap" => provider
            .config
            .get("claudecode_bootstrap")
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "client_data": {},
                    "additional_model_options": [],
                    "additional_model_costs": {},
                    "model_access": [],
                    "org_model_default": null,
                    "oauth_account": {
                        "account_uuid": me.account_uuid,
                        "account_email": format!("{}@gproxy.invalid", me.name),
                        "organization_uuid": me.organization_uuid,
                        "organization_name": provider.id,
                        "organization_type": "api",
                        "organization_rate_limit_tier": tier(&ctx.account.credential),
                    },
                    "auto_compact_windows": {},
                    "narrowed": false,
                })
            }),
        // `validate` and `cri`: capability objects the CLI passes through;
        // nothing about the account is claimed.
        _ => json!({}),
    }
}

/// RFC 3339 UTC from Unix milliseconds, without the `time` formatting
/// feature (Howard Hinnant's civil-from-days).
fn rfc3339(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
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

fn window_payload(window: &CallerUsageWindow) -> Value {
    json!({
        "utilization": window.used_percent.map(|p| p.clamp(0.0, 100.0)),
        "resets_at": window.reset_at_ms.map(rfc3339),
    })
}

/// `GET /api/oauth/usage` in the shape the CLI parses: `five_hour` and
/// `seven_day` from the caller's windows, present only when the host allots
/// them; extra usage off; no scoped limits.
fn usage_answer(usage: &CallerUsage) -> Value {
    let mut response = serde_json::Map::new();
    if let Some(window) = select_window(
        &usage.windows,
        &["primary", "5h", "five_hour", "five-hour"],
        FIVE_HOURS_MS,
    ) {
        response.insert("five_hour".into(), window_payload(window));
    }
    if let Some(window) = select_window(
        &usage.windows,
        &["secondary", "7d", "seven_day", "seven-day"],
        SEVEN_DAYS_MS,
    ) {
        response.insert("seven_day".into(), window_payload(window));
    }
    response.insert("extra_usage".into(), json!({"is_enabled": false}));
    response.insert("limits".into(), json!([]));
    Value::Object(response)
}

/// Neutral defaults, overridable from provider config (v3 key names).
/// `policy_limits` answers 404, which the CLI documents as "unrestricted
/// defaults"; that is the honest answer for a synthesized view.
fn settings_answer(path: &str, config: &Value) -> WireResponse<HttpBody> {
    let value = match path {
        "/api/claude_code/policy_limits" => {
            return not_found("no policy limits configured");
        }
        "/api/claude_code/skills" => json!({
            "skills": config
                .get("claudecode_skill_health")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        }),
        "/api/claude_code_penguin_mode" => config
            .get("claudecode_fast_mode")
            .cloned()
            .unwrap_or_else(|| json!({"enabled": false, "disabled_reason": "preference"})),
        "/api/claude_code_grove" | "/api/oauth/account/settings" => {
            json!({"grove_enabled": false})
        }
        "/api/organization/claude_code_first_token_date" => json!({"first_token_date": null}),
        "/api/claude_code/organizations/metrics_enabled" => json!({"enabled": false}),
        path if path.contains("/mcp/connectors/") => json!({"results": []}),
        _ => json!({}),
    };
    json_response(StatusCode::OK, &value)
}

// ------------------------------------------------------------- resources

fn list_answer(kind: &str, path: &str, config: &Value, mut items: Vec<Value>) -> Value {
    if kind == KIND_SKILL {
        items.extend(
            config
                .get("claudecode_shared_skills")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
    }
    match (kind, path.ends_with("/search")) {
        (_, true) => json!({"results": items}),
        (KIND_PLUGIN, false) => json!({"plugins": items, "has_more": false}),
        _ => json!({"skills": items}),
    }
}

fn created_id(kind: &str) -> fn(&Value) -> Option<String> {
    match kind {
        KIND_FILE => |v| {
            v.get("file_uuid")
                .and_then(Value::as_str)
                .map(str::to_owned)
        },
        _ => |v| v.get("id").and_then(Value::as_str).map(str::to_owned),
    }
}

fn item_param(kind: &str) -> &'static str {
    match kind {
        KIND_FILE => "file_uuid",
        KIND_SKILL => "skill_id",
        _ => "plugin_id",
    }
}

/// Org-scoped item routes carry the synthesized organization in the path;
/// the binding's summary remembers the real one the resource lives under.
fn real_org_path(path: &str, params: &[(&'static str, &str)], summary: &Value) -> String {
    match (
        param(params, "org"),
        summary.get("organization_uuid").and_then(Value::as_str),
    ) {
        (Some(synthetic), Some(real)) if synthetic != real => {
            path.replacen(&format!("/{synthetic}/"), &format!("/{real}/"), 1)
        }
        _ => path.to_owned(),
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
            let ServiceContext {
                account,
                caller,
                request,
                ..
            } = ctx;
            let body = if request.path.ends_with("/search") {
                json_of(&read_body(request.body).await?)
            } else {
                None
            };
            let items = caller
                .list_bindings(kind)
                .await?
                .into_iter()
                .map(|record| record.summary)
                .collect();
            let items = keyword_filter(items, &keywords(body.as_ref()), &["name", "description"]);
            Ok(json_response(
                StatusCode::OK,
                &list_answer(kind, &request.path, account.provider.config, items),
            ))
        }
        Create => {
            let account = ctx.account;
            let upstream = forward(&account, ctx.request)?;
            create_bound(ctx.caller, kind, &account, upstream, created_id(kind), None).await
        }
        Item | ResourceAccess::Delete => {
            let Some(id) = param(params, item_param(kind)) else {
                return Ok(not_found("resource not found"));
            };
            let Some(binding) = ctx.caller.find_binding(kind, id).await? else {
                return Ok(not_found("resource not found"));
            };
            let Some(account) = ctx.account_for(&binding.credential_id) else {
                return Ok(not_found("resource not found"));
            };
            let mut request = ctx.request;
            request.path = real_org_path(&request.path, params, &binding.summary);
            let response = send(&account, request).await?;
            if access == ResourceAccess::Delete && response.status.is_success() {
                ctx.caller.delete_binding(kind, id).await?;
            }
            Ok(response)
        }
    }
}

// ------------------------------------------------------------- dispatch

impl ChannelServices for Claudecode {
    fn routes(&self) -> &[ServiceRoute] {
        routes()
    }

    fn call<'a>(&'a self, ctx: ServiceContext<'a>) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if !is_vendor_path(&ctx.request.path) {
                return Err(ChannelError::UnsupportedService);
            }
            let path = ctx.request.path.clone();
            let Some((route, params)) = classify(routes(), &ctx.request.method, &path) else {
                return Err(ChannelError::UnsupportedService);
            };
            let raw = !ctx.view.is_synthesized();
            match route.class {
                Refused => Ok(forbidden(
                    "creating an API key on the shared Claude account is not allowed through the gateway",
                )),
                Catalog => send(&ctx.account, ctx.request).await,
                _ if raw => send(&ctx.account, ctx.request).await,
                Telemetry => Ok(json_response(StatusCode::OK, &json!({}))),
                Restricted => Ok(forbidden(
                    "this operation acts on the shared account and is not available in this view",
                )),
                Prefix => Ok(not_found("unknown route")),
                Identity => Ok(json_response(StatusCode::OK, &identity_answer(&path, &ctx))),
                Usage => {
                    let usage = ctx.caller.usage().await?;
                    Ok(json_response(StatusCode::OK, &usage_answer(&usage)))
                }
                Settings => Ok(settings_answer(&path, ctx.account.provider.config)),
                Resource { kind, access } => resource_call(kind, access, &params, ctx).await,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_vendor_prefix_is_classified() {
        assert!(is_vendor_path("/api/oauth/profile"));
        assert!(is_vendor_path("/api/oauth/brand_new_in_a_later_cli"));
        assert!(!is_vendor_path("/api/"));
        assert!(!is_vendor_path("/v1/messages"));
        assert!(!is_vendor_path("/apix/oauth/profile"));
    }

    #[test]
    fn every_listed_route_is_classified_under_the_vendor_prefix() {
        for route in routes() {
            assert!(
                is_vendor_path(route.path_template),
                "{}",
                route.path_template
            );
            let probe = route.path_template.replace("{*path}", "x/y");
            let (matched, _) =
                classify(routes(), &route.method, &probe).expect("declared route matches itself");
            assert_eq!(matched.class, route.class, "{}", route.path_template);
        }
        let (route, _) = classify(routes(), &Method::GET, "/api/oauth/usage").unwrap();
        assert_eq!(route.class, Usage);
        let (route, _) = classify(routes(), &Method::GET, "/api/oauth/new_thing").unwrap();
        assert_eq!(route.class, Prefix);
        let (route, _) = classify(
            routes(),
            &Method::POST,
            "/api/oauth/claude_cli/create_api_key",
        )
        .unwrap();
        assert_eq!(route.class, Refused);
    }

    #[test]
    fn timestamps_render_as_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_788_188_400_000), "2026-08-31T15:00:00Z");
        assert_eq!(rfc3339(951_782_400_000), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn org_paths_follow_the_binding() {
        let params = [("org", "synthetic-org"), ("skill_id", "s1")];
        assert_eq!(
            real_org_path(
                "/api/oauth/organizations/synthetic-org/skills/s1/download",
                &params,
                &json!({"organization_uuid": "real-org"})
            ),
            "/api/oauth/organizations/real-org/skills/s1/download"
        );
        assert_eq!(
            real_org_path("/api/x/synthetic-org/y", &params, &json!({})),
            "/api/x/synthetic-org/y"
        );
    }
}
