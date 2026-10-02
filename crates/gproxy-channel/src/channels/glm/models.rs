//! ZCode 3.14.3, commit 29628c9acdb81b703bbd4080c207a0e7ce5e276e:
//! packages/provider-node/src/zcode-builtin-download.ts
//! The public client config resolves the current CDN catalogue URL. Neither
//! request receives the API credential. No model IDs or fallback list live here.

use super::{DEFAULT_BASE_URL, Glm};
use crate::channel::{ChannelError, OperationContext};
use crate::channels::shared::compatible::ability::{require_success, send};
use gproxy_protocol::{HttpBody, WireResponse};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};

const CONFIG_URL: &str =
    "https://zcode.z.ai/api/v1/client/configs?app_version=3.14.3&platform=linux-x64";

pub(super) async fn reply(
    channel: &Glm,
    ctx: OperationContext<'_>,
    single: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let invalid = |message: &str| ChannelError::InvalidResponse(message.into());
    let get = async |url: &str| -> Result<Value, ChannelError> {
        let (status, _, body) = send(
            ctx.client.as_ref(),
            Method::GET,
            url,
            HeaderMap::new(),
            None,
        )
        .await?;
        serde_json::from_slice(&require_success(status, body)?)
            .map_err(|e| ChannelError::InvalidResponse(e.to_string()))
    };
    let config = get(ctx.endpoint_override.unwrap_or(CONFIG_URL)).await?;
    if config.get("code").and_then(Value::as_i64) != Some(0) {
        return Err(invalid("ZCode client config request failed"));
    }
    let url = config
        .pointer("/data/configs/builtin_provider_config_json")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("ZCode client config missing catalogue URL"))?;
    let catalogue = get(url).await?;
    let templates = catalogue
        .pointer("/config/providerConfigRules/templateRules")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("ZCode catalogue missing provider templates"))?;
    let base: http::Uri = ctx
        .provider
        .base_url
        .unwrap_or(DEFAULT_BASE_URL)
        .parse()
        .map_err(|e: http::uri::InvalidUri| ChannelError::InvalidConfig(e.to_string()))?;
    let family = if base.host() == Some("api.z.ai") {
        "zai"
    } else {
        "bigmodel"
    };
    let template_id = format!(
        "{family}-{}",
        if channel.coding_plan {
            "api"
        } else {
            "standard-api"
        }
    );
    let ids = templates
        .iter()
        .find(|t| t.get("templateId").and_then(Value::as_str) == Some(&template_id))
        .and_then(|t| t.pointer("/config/builtinModelIds"))
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("ZCode catalogue missing GLM model list"))?;
    let mut models = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id
            .as_str()
            .ok_or_else(|| invalid("ZCode model ID is not a string"))?;
        models.push(
            json!({"id":id,"object":"model","created":0,"owned_by":family,
            "catalog_source":"zcode","catalog_revision":catalogue.get("revision")}),
        );
    }
    let (status, body) = if single {
        let id = ctx.request.path.rsplit('/').next().unwrap_or_default();
        match models.into_iter().find(|model| {
            model["id"]
                .as_str()
                .is_some_and(|m| m.eq_ignore_ascii_case(id))
        }) {
            Some(model) => (StatusCode::OK, model),
            None => (
                StatusCode::NOT_FOUND,
                json!({"error":{"type":"not_found_error","message":"Model is not in the upstream GLM catalogue"}}),
            ),
        }
    } else {
        (StatusCode::OK, json!({"object":"list","data":models}))
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(body.to_string().into()),
    })
}
