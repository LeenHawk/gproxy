//! Shared regional endpoint resolution for Amazon Bedrock channels.

use crate::channel::http_util::{exact_url, join_url};
use crate::channel::settings::{endpoint_by_key, endpoint_key};
use crate::channel::{ChannelError, PrepareCtx};

const DEFAULT_REGION: &str = "us-east-1";

#[derive(Clone, Copy)]
pub(super) enum Service {
    Mantle,
    Runtime,
}

impl Service {
    fn base_url(self, region: &str) -> String {
        match self {
            Self::Mantle => format!("https://bedrock-mantle.{region}.api.aws"),
            Self::Runtime => format!("https://bedrock-runtime.{region}.amazonaws.com"),
        }
    }
}

pub(super) fn resolve_uri(
    ctx: &PrepareCtx<'_>,
    service: Service,
    fallback_path: &str,
    query: Option<&str>,
    endpoint_model: &str,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = endpoint_by_key(
        ctx.provider_settings,
        endpoint_key(ctx.op, ctx.stream),
        endpoint_model,
    ) {
        return exact_url(&url, query);
    }

    let configured_base = ctx
        .provider_settings
        .get("base_url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty());
    let base = match configured_base {
        Some(base) => base.to_owned(),
        None => service.base_url(region(ctx.provider_settings)?),
    };
    join_url(&base, fallback_path, query)
}

fn region(settings: &serde_json::Value) -> Result<&str, ChannelError> {
    let region = settings
        .get("region")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|region| !region.is_empty())
        .unwrap_or(DEFAULT_REGION);
    if region
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(region)
    } else {
        Err(ChannelError::Build("invalid AWS region".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, Method};
    use serde_json::json;

    #[test]
    fn rejects_region_that_could_change_the_host() {
        let secret = json!({});
        let settings = json!({ "region": "us-east-1/other" });
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: crate::protocol::OperationKey::provider(
                crate::protocol::Operation::ListModels,
                crate::protocol::Provider::OpenAi,
            ),
            stream: false,
            upstream_model_id: "",
            method: Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        };
        assert!(resolve_uri(&ctx, Service::Runtime, ctx.path, None, "").is_err());
    }
}
