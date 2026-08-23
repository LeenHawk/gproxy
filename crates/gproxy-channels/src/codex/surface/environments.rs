use std::collections::BTreeMap;

use gproxy_channel_api::{
    BoxFuture, ChannelError, SurfaceReply, SurfaceServices, SynthCtx, Synthesizer,
};
use http::{Method, StatusCode};
use serde_json::Value;

use super::helpers::{canonical_path, invoke, json_reply, reply_json, request};

pub(super) static HANDLER: Environments = Environments;

pub(super) struct Environments;

impl Synthesizer for Environments {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            let canonical = canonical_path(ctx.path);
            let rest = canonical
                .strip_prefix("/api/codex/")
                .ok_or_else(|| ChannelError::Prepare("environment path is not canonical".into()))?;
            let mut merged = BTreeMap::<String, Value>::new();
            for credential in services.credentials {
                let Ok(reply) = invoke(
                    &services,
                    request(
                        "environments",
                        Method::GET,
                        format!("/wham/{rest}"),
                        ctx.query,
                        ctx.headers,
                        Default::default(),
                        Some(*credential),
                    ),
                )
                .await
                else {
                    continue;
                };
                if !reply.status.is_success() {
                    continue;
                }
                let Ok(Value::Array(items)) = reply_json(&reply) else {
                    continue;
                };
                for item in items {
                    if let Some(id) = item.get("id").and_then(Value::as_str).map(str::to_owned) {
                        merged.entry(id).or_insert(item);
                    }
                }
            }
            Ok(json_reply(
                StatusCode::OK,
                Value::Array(merged.into_values().collect()),
            ))
        })
    }
}
