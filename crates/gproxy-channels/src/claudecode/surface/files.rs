use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, ChannelError, SurfaceReply, SurfaceServices, SynthCtx, Synthesizer,
};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::helpers::{
    FILE_KIND, FILES_BETA, delete_resource, invoke, json_reply, param, reply_json, request,
    resource_headers, safe_query, save_resource,
};
use super::pagination::{list_resources, paginate};

pub(super) static HANDLER: Files = Files;

pub(super) struct Files;

impl Synthesizer for Files {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            if ctx.path == "/api/oauth/file_upload" {
                return oauth_upload(ctx, &services).await;
            }
            if ctx.path == "/v1/files" && *ctx.method == Method::GET {
                let resources = list_resources(&services, FILE_KIND, ctx.query).await?;
                return Ok(json_reply(StatusCode::OK, paginate(resources)));
            }
            if ctx.path == "/v1/files" {
                return create(ctx, &services).await;
            }
            delete(ctx, &services).await
        })
    }
}

async fn oauth_upload(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let reply = invoke(
        services,
        request(
            "claude_oauth_file_upload",
            Method::POST,
            "/v1/files".to_owned(),
            None,
            resource_headers(ctx.headers, FILES_BETA),
            ctx.body.clone(),
            services.credential,
        ),
    )
    .await?;
    if !reply.status.is_success() {
        return Ok(reply);
    }
    let value = reply_json(&reply)?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Decode("Claude file response missing id".into()))?;
    save_resource(services, FILE_KIND, id, services.credential, value.clone()).await?;
    Ok(json_reply(StatusCode::CREATED, json!({ "file_uuid": id })))
}

async fn create(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let reply = invoke(
        services,
        request(
            "claude_file_create",
            Method::POST,
            "/v1/files".to_owned(),
            safe_query(ctx.query),
            resource_headers(ctx.headers, FILES_BETA),
            ctx.body.clone(),
            services.credential,
        ),
    )
    .await?;
    if reply.status.is_success() {
        let value = reply_json(&reply)?;
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            save_resource(services, FILE_KIND, id, services.credential, value.clone()).await?;
        }
    }
    Ok(reply)
}

async fn delete(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let id = param(ctx.params, "file_id")?;
    let reply = invoke(
        services,
        request(
            "claude_file_delete",
            Method::DELETE,
            format!("/v1/files/{id}"),
            safe_query(ctx.query),
            resource_headers(ctx.headers, FILES_BETA),
            Bytes::new(),
            services.credential,
        ),
    )
    .await?;
    if reply.status.is_success() {
        delete_resource(services, FILE_KIND, id).await?;
    }
    Ok(reply)
}
