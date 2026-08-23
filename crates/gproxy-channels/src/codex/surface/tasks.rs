use gproxy_channel_api::{
    BoxFuture, ChannelError, CredentialId, Page, SurfaceReply, SurfaceServices, SynthCtx,
    Synthesizer,
};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::helpers::{
    TASK_KIND, canonical_path, invoke, json_reply, param, query_pairs, query_value, reply_json,
    request, save_binding, unix_now,
};

pub(super) static HANDLER: Tasks = Tasks;

pub(super) struct Tasks;

impl Synthesizer for Tasks {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            let path = canonical_path(ctx.path);
            if *ctx.method == Method::GET && path == "/api/codex/tasks/list" {
                return list(ctx.query, &services).await;
            }
            if *ctx.method == Method::POST && path == "/api/codex/tasks" {
                return create(ctx, &services).await;
            }
            bound(ctx, &services, &path).await
        })
    }
}

async fn list(
    query: Option<&str>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let pairs = query_pairs(query);
    let limit = query_value(&pairs, "limit")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let environment = query_value(&pairs, "environment_id");
    let task_filter = query_value(&pairs, "task_filter");
    let page = services
        .bindings
        .list(
            services.provider.id,
            services.identity.user_id,
            TASK_KIND,
            Page {
                cursor: query_value(&pairs, "cursor").map(str::to_owned),
                limit,
            },
        )
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    let items = page
        .items
        .into_iter()
        .filter(|row| {
            environment.is_none_or(|id| {
                row.summary.get("environment_id").and_then(Value::as_str) == Some(id)
            }) && task_filter.is_none_or(|filter| match filter {
                "archived" => row.summary.get("archived").and_then(Value::as_bool) == Some(true),
                "current" => row.summary.get("archived").and_then(Value::as_bool) != Some(true),
                _ => true,
            })
        })
        .map(|row| row.summary)
        .collect::<Vec<_>>();
    Ok(json_reply(
        StatusCode::OK,
        json!({"items":items,"cursor":page.next_cursor}),
    ))
}

async fn create(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
) -> Result<SurfaceReply, ChannelError> {
    let value = serde_json::from_slice::<Value>(ctx.body)
        .map_err(|error| ChannelError::Prepare(format!("task request JSON: {error}")))?;
    let credential = match value
        .pointer("/new_task/environment_id")
        .and_then(Value::as_str)
    {
        Some(environment_id) => {
            environment_credential(services, environment_id, ctx.headers).await?
        }
        None => services.credential,
    };
    let reply = invoke(
        services,
        request(
            "task_create",
            Method::POST,
            "/wham/tasks".into(),
            ctx.query,
            ctx.headers,
            ctx.body.clone(),
            Some(credential),
        ),
    )
    .await?;
    if reply.status.is_success() {
        let response = reply_json(&reply)?;
        let task_id = response
            .pointer("/task/id")
            .or_else(|| response.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| ChannelError::Decode("task response missing id".into()))?;
        let environment_id = value
            .pointer("/new_task/environment_id")
            .and_then(Value::as_str);
        let now = unix_now();
        save_binding(
            services,
            TASK_KIND,
            task_id,
            credential,
            json!({
                "id":task_id,
                "title":task_title(&value),
                "environment_id":environment_id,
                "has_generated_title":false,
                "updated_at":now,
                "created_at":now,
                "task_status_display":null,
                "archived":false,
                "has_unread_turn":false,
                "pull_requests":null
            }),
        )
        .await?;
    }
    Ok(reply)
}

async fn environment_credential(
    services: &SurfaceServices<'_>,
    environment_id: &str,
    headers: &http::HeaderMap,
) -> Result<CredentialId, ChannelError> {
    for credential in services.credentials {
        let Ok(reply) = invoke(
            services,
            request(
                "task_environment_discovery",
                Method::GET,
                "/wham/environments".into(),
                None,
                headers,
                Default::default(),
                Some(*credential),
            ),
        )
        .await
        else {
            continue;
        };
        if reply.status.is_success()
            && reply_json(&reply)?.as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.get("id").and_then(Value::as_str) == Some(environment_id))
            })
        {
            return Ok(*credential);
        }
    }
    Err(ChannelError::Prepare(
        "requested environment has no eligible credential".into(),
    ))
}

async fn bound(
    ctx: SynthCtx<'_>,
    services: &SurfaceServices<'_>,
    canonical: &str,
) -> Result<SurfaceReply, ChannelError> {
    let task_id = param(ctx.params, "task_id")?;
    let rest = canonical
        .strip_prefix("/api/codex/")
        .ok_or_else(|| ChannelError::Prepare("task path is not canonical".into()))?;
    let reply = invoke(
        services,
        request(
            "task_bound",
            ctx.method.clone(),
            format!("/wham/{rest}"),
            ctx.query,
            ctx.headers,
            ctx.body.clone(),
            Some(services.credential),
        ),
    )
    .await?;
    if *ctx.method == Method::GET && reply.status.is_success() {
        let value = reply_json(&reply)?;
        let binding = super::helpers::find_binding(services, TASK_KIND, task_id).await?;
        let mut summary = binding.summary;
        if let Some(status) = value
            .get("task_status_display")
            .or_else(|| value.pointer("/task/task_status_display"))
        {
            summary["task_status_display"] = status.clone();
        }
        summary["updated_at"] = Value::from(unix_now());
        save_binding(services, TASK_KIND, task_id, binding.credential, summary).await?;
    }
    Ok(reply)
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
        .find_map(|part| part.get("text").and_then(Value::as_str))
        .unwrap_or("Codex task")
        .chars()
        .take(120)
        .collect()
}
