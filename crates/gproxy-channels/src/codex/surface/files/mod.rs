mod multipart;
mod upload;

use gproxy_channel_api::{
    BoxFuture, ChannelError, Page, SurfaceReply, SurfaceServices, SynthCtx, Synthesizer,
};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::helpers::{
    FILE_KIND, canonical_path, file_deleted, find_binding, json_reply, query_pairs, query_value,
    save_binding, transport_reply,
};

pub(super) static HANDLER: Files = Files;

pub(super) struct Files;

impl Synthesizer for Files {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            let path = canonical_path(ctx.path);
            match (ctx.method.as_str(), path.as_str()) {
                ("GET", "/v1/files") => list(ctx.query, &services).await,
                ("POST", "/api/codex/files") => upload::hosted_create(ctx, &services).await,
                ("POST", "/v1/files") => upload::openai_create(ctx, &services).await,
                _ => resource(ctx, &services, &path).await,
            }
        })
    }
}

async fn list(
    query: Option<&str>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let pairs = query_pairs(query);
    let purpose = query_value(&pairs, "purpose");
    let after = query_value(&pairs, "after");
    let limit = query_value(&pairs, "limit")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(10_000)
        .clamp(1, 10_000);
    let mut rows = services
        .bindings
        .list(
            services.provider.id,
            services.identity.user_id,
            FILE_KIND,
            Page {
                cursor: None,
                limit,
            },
        )
        .await
        .map_err(transport_reply)?;
    rows.sort_by_key(|row| std::cmp::Reverse(row.created_at_unix));
    let mut found_after = after.is_none();
    let data = rows
        .into_iter()
        .filter_map(|row| row.summary.get("file").cloned())
        .filter(|file| {
            if !found_after {
                found_after = file.get("id").and_then(Value::as_str) == after;
                return false;
            }
            purpose
                .is_none_or(|purpose| file.get("purpose").and_then(Value::as_str) == Some(purpose))
        })
        .take(limit as usize)
        .collect::<Vec<_>>();
    Ok(json_reply(
        StatusCode::OK,
        json!({"object":"list","data":data,"has_more":false}),
    ))
}

async fn resource(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
    path: &str,
) -> Result<SurfaceReply, ChannelError> {
    let (file_id, action) = file_action(path, ctx.method)?;
    let binding = find_binding(services, FILE_KIND, file_id).await?;
    match action {
        FileAction::Retrieve => Ok(json_reply(
            StatusCode::OK,
            binding.summary.get("file").cloned().unwrap_or(Value::Null),
        )),
        FileAction::Delete => {
            services
                .bindings
                .delete(
                    services.provider.id,
                    services.identity.user_id,
                    FILE_KIND,
                    file_id,
                )
                .await
                .map_err(transport_reply)?;
            Ok(json_reply(StatusCode::OK, file_deleted(file_id)))
        }
        FileAction::Finalize => {
            let value = upload::finalize(services, file_id, binding.credential).await?;
            let mut summary = binding.summary;
            summary["hosted"] = value.clone();
            summary["file"]["status"] = Value::String("processed".into());
            save_binding(services, FILE_KIND, file_id, binding.credential, summary).await?;
            Ok(json_reply(StatusCode::OK, value))
        }
        FileAction::Content => upload::content(services, file_id, binding.credential).await,
    }
}

enum FileAction {
    Retrieve,
    Delete,
    Content,
    Finalize,
}

fn file_action<'a>(path: &'a str, method: &Method) -> Result<(&'a str, FileAction), ChannelError> {
    if let Some(id) = path
        .strip_prefix("/api/codex/files/")
        .and_then(|rest| rest.strip_suffix("/uploaded"))
        .filter(|id| !id.is_empty() && !id.contains('/'))
    {
        return (*method == Method::POST)
            .then_some((id, FileAction::Finalize))
            .ok_or_else(|| ChannelError::Prepare("unsupported hosted file method".into()));
    }
    let rest = path
        .strip_prefix("/v1/files/")
        .ok_or_else(|| ChannelError::Prepare("unsupported file path".into()))?;
    if let Some(id) = rest
        .strip_suffix("/content")
        .filter(|id| !id.is_empty() && !id.contains('/'))
    {
        return (*method == Method::GET)
            .then_some((id, FileAction::Content))
            .ok_or_else(|| ChannelError::Prepare("unsupported file content method".into()));
    }
    if rest.is_empty() || rest.contains('/') {
        return Err(ChannelError::Prepare(
            "unsupported file resource path".into(),
        ));
    }
    match method.as_str() {
        "GET" => Ok((rest, FileAction::Retrieve)),
        "DELETE" => Ok((rest, FileAction::Delete)),
        _ => Err(ChannelError::Prepare(
            "unsupported file resource method".into(),
        )),
    }
}
