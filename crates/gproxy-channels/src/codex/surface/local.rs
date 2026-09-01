use gproxy_channel_api::{
    BoxFuture, ChannelError, SurfaceReply, SurfaceServices, SynthCtx, Synthesizer,
};
use http::StatusCode;
use serde_json::{Value, json};

use super::helpers::{canonical_path, json_reply, plan_type, stable_id, user_name};
use super::usage::{profile, usage};

pub(super) static HANDLER: Local = Local;

pub(super) struct Local;

impl Synthesizer for Local {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            let path = canonical_path(ctx.path);
            if *ctx.method == http::Method::GET && path == "/api/codex/remote/control/server" {
                return Ok(json_reply(
                    StatusCode::UPGRADE_REQUIRED,
                    json!({"error":{"message":"websocket upgrade required"}}),
                ));
            }
            if *ctx.method == http::Method::GET
                && ctx.params.iter().any(|(name, _)| *name == "account_id")
                && path.ends_with("/settings")
            {
                return workspace_settings(ctx, &services);
            }
            let value = match (ctx.method.as_str(), path.as_str()) {
                ("GET", "/v1/user-auth-credential/whoami") => whoami(&services),
                ("GET", "/api/codex/usage") => usage(&services).await?,
                ("POST", "/api/codex/usage/thread_usage/query") => {
                    // UsageView is caller/provider scoped but has no thread
                    // dimension. Returning no fabricated groups is honest.
                    json!({"threads": []})
                }
                ("GET", "/api/codex/accounts/check") => account(&services),
                ("GET", "/api/codex/profiles/me") => profile(&services).await?,
                ("GET", "/api/codex/settings/user") => services
                    .provider
                    .settings
                    .get("codex_virtual_settings")
                    .cloned()
                    .unwrap_or_else(|| json!({"commit_attribution_enabled":false})),
                ("GET", "/api/codex/workspace-messages") => json!({
                    "messages": services.provider.settings
                        .get("codex_workspace_messages")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                }),
                ("GET", "/api/codex/config/bundle") => services
                    .provider
                    .settings
                    .get("codex_config_bundle")
                    .cloned()
                    .unwrap_or_else(|| json!({"config_toml":null,"requirements_toml":null})),
                ("GET", "/api/codex/rate-limit-reset-credits") => {
                    json!({"available_count":0,"credits":[]})
                }
                ("POST", "/api/codex/rate-limit-reset-credits/consume") => {
                    json!({"code":"no_credit","windows_reset":0})
                }
                ("POST", "/api/codex/accounts/send_add_credits_nudge_email")
                | ("POST", "/api/codex/analytics-events/events")
                | ("POST", "/v1/analytics/codex/turn-costs")
                | ("POST", "/api/codex/analytics/codex/turn-costs") => json!({}),
                _ => {
                    return Err(ChannelError::Prepare(format!(
                        "unsupported local Codex surface: {} {path}",
                        ctx.method
                    )));
                }
            };
            Ok(json_reply(StatusCode::OK, value))
        })
    }
}

fn workspace_settings(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    ctx.params
        .iter()
        .find_map(|(name, value)| (*name == "account_id").then_some(value.as_str()))
        .ok_or_else(|| ChannelError::Prepare("workspace account id missing".into()))?;
    let mut beta_settings = serde_json::Map::new();
    if let Some(enabled) = services
        .provider
        .settings
        .get("codex_plugins_enabled")
        .and_then(Value::as_bool)
    {
        beta_settings.insert("enable_plugins".into(), Value::Bool(enabled));
    }
    Ok(json_reply(
        StatusCode::OK,
        json!({"beta_settings":beta_settings}),
    ))
}

fn ids(services: &SurfaceServices<'_>) -> (String, String) {
    (
        stable_id("user", services.provider.id, services.identity.user_id),
        stable_id("account", services.provider.id, services.identity.user_id),
    )
}

fn whoami(services: &SurfaceServices<'_>) -> Value {
    let (user_id, account_id) = ids(services);
    json!({
        "email": null,
        "chatgpt_user_id": user_id,
        "chatgpt_account_id": account_id,
        "chatgpt_plan_type": plan_type(services.provider.settings),
        "chatgpt_account_is_fedramp": false
    })
}

fn account(services: &SurfaceServices<'_>) -> Value {
    let (_, account_id) = ids(services);
    json!({
        "accounts": [{
            "id": account_id,
            "name": user_name(services),
            "profile_picture_url": null,
            "structure": plan_type(services.provider.settings)
        }],
        "account_ordering": [account_id],
        "default_account_id": account_id
    })
}
