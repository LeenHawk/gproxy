use super::{Config, Edition, Volcengine, control, json_response};
use crate::channel::{ChannelError, CredentialContext, OperationContext};
use crate::channels::shared::compatible::http::invalid_response;
use gproxy_protocol::{HttpBody, WireResponse};
use http::StatusCode;
use serde_json::{Value, json};

pub(super) async fn reply(
    channel: &Volcengine,
    ctx: OperationContext<'_>,
    single: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let config = Config::read(ctx.provider)?;
    let (action, body) = if channel.agent {
        (
            "ListArkAgentPlanModel",
            json!({"Edition": match config.edition { Edition::Personal => "personal", Edition::Enterprise => "enterprise" }}),
        )
    } else {
        ("ListArkCodingPlanModel", json!({}))
    };
    let result = control::call(
        CredentialContext {
            provider: ctx.provider,
            credential: ctx.credential,
            client: ctx.client.as_ref(),
        },
        action,
        body,
        ctx.endpoint_override,
    )
    .await?;
    let models = result
        .get("Datas")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("Ark catalogue missing Datas"))?
        .iter()
        .map(|model| {
            let id = model
                .get("ModelID")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid_response("Ark catalogue missing ModelID"))?;
            Ok(json!({"id": id, "object": "model", "created": 0, "owned_by": "volcengine"}))
        })
        .collect::<Result<Vec<_>, ChannelError>>()?;
    Ok(if single {
        let id = ctx
            .request
            .path
            .split_once("/models/")
            .map(|(_, id)| id)
            .ok_or_else(|| ChannelError::InvalidConfig("model id missing".into()))?;
        let id = percent_encoding::percent_decode_str(id)
            .decode_utf8()
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
        match models
            .into_iter()
            .find(|model| model["id"].as_str() == Some(id.as_ref()))
        {
            Some(model) => json_response(StatusCode::OK, model),
            None => json_response(
                StatusCode::NOT_FOUND,
                json!({"error": {"type": "not_found_error", "message": "Model is not in the Ark plan catalogue"}}),
            ),
        }
    } else {
        json_response(StatusCode::OK, json!({"object": "list", "data": models}))
    })
}
