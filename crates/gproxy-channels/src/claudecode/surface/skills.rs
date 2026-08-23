use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, ChannelError, SurfaceReply, SurfaceServices, SynthCtx, Synthesizer,
};
use http::{Method, StatusCode};
use serde_json::Value;

use super::helpers::{
    SKILL_KIND, SKILLS_BETA, delete_resource, invoke, json_reply, param, reply_json, request,
    resource_headers, save_resource, skills_query,
};
use super::pagination::{list_resources, paginate};

pub(super) static HANDLER: Skills = Skills;

pub(super) struct Skills;

impl Synthesizer for Skills {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            if ctx.path == "/v1/skills" && *ctx.method == Method::GET {
                let resources = list_resources(&services, SKILL_KIND, ctx.query).await?;
                return Ok(json_reply(StatusCode::OK, paginate(resources)));
            }
            if ctx.path == "/v1/skills" {
                return create(ctx, &services).await;
            }
            if ctx.path.ends_with("/versions") {
                return create_version(ctx, &services).await;
            }
            delete(ctx, &services).await
        })
    }
}

async fn create(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let reply = invoke(
        services,
        request(
            "claude_skill_create",
            Method::POST,
            "/v1/skills".to_owned(),
            Some(skills_query(ctx.query)),
            resource_headers(ctx.headers, SKILLS_BETA),
            ctx.body.clone(),
            services.credential,
        ),
    )
    .await?;
    if reply.status.is_success() {
        let value = reply_json(&reply)?;
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            save_resource(services, SKILL_KIND, id, services.credential, value.clone()).await?;
        }
    }
    Ok(reply)
}

async fn delete(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let skill_id = param(ctx.params, "skill_id")?;
    let reply = invoke(
        services,
        request(
            "claude_skill_delete",
            Method::DELETE,
            format!("/v1/skills/{skill_id}"),
            Some(skills_query(ctx.query)),
            resource_headers(ctx.headers, SKILLS_BETA),
            Bytes::new(),
            services.credential,
        ),
    )
    .await?;
    if reply.status.is_success() {
        delete_resource(services, SKILL_KIND, skill_id).await?;
    }
    Ok(reply)
}

async fn create_version(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let skill_id = param(ctx.params, "skill_id")?;
    let reply = invoke(
        services,
        request(
            "claude_skill_version_create",
            Method::POST,
            format!("/v1/skills/{skill_id}/versions"),
            Some(skills_query(ctx.query)),
            resource_headers(ctx.headers, SKILLS_BETA),
            ctx.body.clone(),
            services.credential,
        ),
    )
    .await?;
    if reply.status.is_success() {
        update_version_summary(services, skill_id, reply_json(&reply)?).await?;
    }
    Ok(reply)
}

async fn update_version_summary(
    services: &SurfaceServices<'_>,
    skill_id: &str,
    version: Value,
) -> Result<(), ChannelError> {
    let binding = services
        .bindings
        .find(
            services.provider.id,
            services.identity.user_id,
            SKILL_KIND,
            skill_id,
        )
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))?
        .ok_or_else(|| ChannelError::Prepare("skill binding disappeared".into()))?;
    let mut summary = binding.summary;
    if let Some(description) = version.get("description") {
        summary["resource"]["description"] = description.clone();
    }
    if let Some(version_id) = version.get("id") {
        summary["resource"]["latest_version_id"] = version_id.clone();
    }
    services
        .bindings
        .save(
            services.provider.id,
            services.identity.user_id,
            SKILL_KIND,
            skill_id,
            binding.credential,
            summary,
        )
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
