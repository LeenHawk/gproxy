//! The native GetConfigDetail API returns config_info_list. Its config_name is
//! the public model identity; model_detail_list binds it to backend model params.
use super::{
    common::{check, invalid, json},
    config::TraeConfig,
    request,
};
use crate::channel::{ChannelError, CredentialView, OperationContext, ProviderView};
use crate::channels::shared::compatible::ability::{require_success, send};
use gproxy_protocol::{HttpBody, WireResponse, connection::Bytes};
use http::{Method, header};
use serde_json::{Value, json};

pub(super) fn url(
    config: &TraeConfig,
    provider: ProviderView<'_>,
    credential: &CredentialView<'_>,
) -> Result<String, ChannelError> {
    let mut url = url::Url::parse(&format!(
        "{}/api/ide/v1/get_detail_param",
        config.api(provider, credential)
    ))
    .map_err(invalid)?;
    url.query_pairs_mut().extend_pairs([
        ("function", "chat_v3"),
        ("need_prompt", "false"),
        ("poly_prompt", "true"),
    ]);
    Ok(url.into())
}

pub(super) fn configs(value: &Value) -> Result<&Vec<Value>, ChannelError> {
    check(value)?;
    value["config_info_list"]
        .as_array()
        .ok_or_else(|| invalid("missing config_info_list"))
}

pub(super) async fn selected(
    provider: ProviderView<'_>,
    credential: CredentialView<'_>,
    client: &dyn crate::OutboundClient,
    model: &str,
) -> Result<Value, ChannelError> {
    let config = TraeConfig::from_view(provider)?;
    let (status, _, body) = send(
        client,
        Method::GET,
        &url(&config, provider, &credential)?,
        request::headers(&config, &credential)?,
        None,
    )
    .await?;
    let value = json(&require_success(status, body)?)?;
    configs(&value)?
        .iter()
        .find(|v| v["config_name"].as_str() == Some(model) && !is_custom(v))
        .cloned()
        .ok_or_else(|| {
            ChannelError::InvalidConfig(format!(
                "Trae: model {model} is not in this account's chat catalogue"
            ))
        })
}

fn is_custom(value: &Value) -> bool {
    value["is_custom_model"].as_bool() == Some(true)
        || value
            .get("custom_models")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
}

pub(super) async fn list(
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let config = TraeConfig::from_view(ctx.provider)?;
    let target = ctx
        .endpoint_override
        .map(str::to_owned)
        .map(Ok)
        .unwrap_or_else(|| url(&config, ctx.provider, &ctx.credential))?;
    let mut req = http::Request::builder()
        .method(Method::GET)
        .uri(target)
        .body(HttpBody::Bytes(Bytes::new()))
        .map_err(invalid)?;
    *req.headers_mut() = request::headers(&config, &ctx.credential)?;
    let response = ctx.client.send(req).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let value = json(&crate::channels::shared::compatible::http::read_body(response.body).await?)?;
    let rows: Vec<_> = configs(&value)?.iter().filter(|v| !is_custom(v)).filter_map(|v| {
        let id = v["config_name"].as_str()?;
        Some(json!({"id":id,"object":"model","created":0,"owned_by":"trae",
            "name":v.pointer("/display_config/display_name").and_then(Value::as_str).unwrap_or(id)}))
    }).collect();
    let mut headers = response.headers;
    headers.remove(header::CONTENT_LENGTH);
    headers.remove(header::CONTENT_ENCODING);
    headers.insert(
        header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    Ok(WireResponse {
        status: response.status,
        headers,
        body: HttpBody::Bytes(Bytes::from(
            json!({"object":"list","data":rows}).to_string(),
        )),
    })
}
