use gproxy_channel_api::{
    BoxFuture, ChannelError, SurfaceReply, SurfaceServices, SynthCtx, Synthesizer,
};
use http::StatusCode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::helpers::{SKILL_KIND, json_reply, list_resources, param};

pub(super) static HANDLER: Local = Local;

pub(super) struct Local;

impl Synthesizer for Local {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            let reply = match ctx.path {
                "/api/hello" => json_reply(StatusCode::OK, json!({})),
                "/api/claude_cli/bootstrap" => json_reply(StatusCode::OK, bootstrap(&services)),
                "/api/claude_cli_profile" => json_reply(StatusCode::OK, profile(&services)),
                "/api/claude_code_penguin_mode" => json_reply(StatusCode::OK, penguin(&services)),
                "/api/claude_code/skills" => json_reply(
                    StatusCode::OK,
                    json!({
                        "skills": services.provider.settings
                            .get("claudecode_skill_health")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default()
                    }),
                ),
                path if path.starts_with("/api/oauth/organizations/") => {
                    oauth_skills(ctx, &services).await?
                }
                _ => {
                    return Err(ChannelError::Prepare(
                        "unknown local Claude Code surface".into(),
                    ));
                }
            };
            Ok(reply)
        })
    }
}

fn stable_id(kind: &str, provider_id: i64, user_id: i64) -> String {
    let digest = Sha256::digest(format!("gproxy-claude-{kind}:{provider_id}:{user_id}"));
    let encoded = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("gproxy-{kind}-{encoded}")
}

fn user_name(services: &SurfaceServices<'_>) -> String {
    format!("user-{}", services.identity.user_id)
}

fn profile(services: &SurfaceServices<'_>) -> Value {
    let name = user_name(services);
    json!({
        "account": {
            "uuid": stable_id("account", services.provider.id, services.identity.user_id),
            "email": format!("{name}@gproxy.invalid"),
            "display_name": name,
        },
        "organization": {
            "uuid": stable_id("organization", services.provider.id, services.identity.user_id),
            "organization_type": "api",
            "rate_limit_tier": "gproxy",
        }
    })
}

fn bootstrap(services: &SurfaceServices<'_>) -> Value {
    if let Some(value) = services.provider.settings.get("claudecode_bootstrap") {
        return value.clone();
    }
    let name = user_name(services);
    json!({
        "client_data": {},
        "additional_model_options": [],
        "additional_model_costs": {},
        "model_access": [],
        "org_model_default": null,
        "oauth_account": {
            "account_uuid": stable_id("account", services.provider.id, services.identity.user_id),
            "account_email": format!("{name}@gproxy.invalid"),
            "organization_uuid": stable_id("organization", services.provider.id, services.identity.user_id),
            "organization_name": services.provider.name,
            "organization_type": "api",
            "organization_rate_limit_tier": "gproxy",
        },
        "auto_compact_windows": {},
        "narrowed": false,
    })
}

fn penguin(services: &SurfaceServices<'_>) -> Value {
    services
        .provider
        .settings
        .get("claudecode_fast_mode")
        .cloned()
        .unwrap_or_else(|| json!({ "enabled": false, "disabled_reason": "preference" }))
}

async fn oauth_skills(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let organization = param(ctx.params, "organization")?;
    let expected = stable_id(
        "organization",
        services.provider.id,
        services.identity.user_id,
    );
    if organization != expected {
        return Ok(json_reply(
            StatusCode::NOT_FOUND,
            json!({ "error": { "type": "not_found_error", "message": "organization not found" } }),
        ));
    }
    if ctx.path.ends_with("/download") {
        return Ok(json_reply(
            StatusCode::NOT_FOUND,
            json!({ "error": { "type": "not_found_error", "message": "skill archive unavailable" } }),
        ));
    }

    let mut skills = list_resources(services, SKILL_KIND, None)
        .await?
        .iter()
        .map(skill_to_oauth)
        .collect::<Vec<_>>();
    skills.extend(
        services
            .provider
            .settings
            .get("claudecode_shared_skills")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    );
    if ctx.path.ends_with("/search") {
        let keywords = serde_json::from_slice::<Value>(ctx.body)
            .ok()
            .and_then(|value| value.get("keywords").and_then(Value::as_array).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_ascii_lowercase))
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
        Ok(json_reply(StatusCode::OK, json!({ "results": skills })))
    } else {
        Ok(json_reply(StatusCode::OK, json!({ "skills": skills })))
    }
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
