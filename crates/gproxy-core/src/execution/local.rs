use bytes::Bytes;
use gproxy_channel_api::Disposition;
use gproxy_protocol::{Operation, OperationKind, SettleMode, WireFamily};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use web_time::Instant;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::host::Host;

use super::request::Classified;

pub(super) async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    request: &RequestCtx,
    plan: &Plan,
    classified: &Classified,
    started: Instant,
) -> Option<Result<ExecOutcome, CoreError>> {
    let operation = classified.key.operation;
    if !matches!(
        operation,
        Operation::ListModels | Operation::GetModel | Operation::CountTokens
    ) {
        return None;
    }
    Some(serve(core, control, request, plan, classified, started).await)
}

async fn serve<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    request: &RequestCtx,
    plan: &Plan,
    classified: &Classified,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let target = plan
        .targets
        .first()
        .cloned()
        .ok_or(CoreError::NoCredentials)?;
    let OperationKind::Family(family) = classified.key.kind else {
        return Err(CoreError::Internal(
            "local operation has a non-family wire kind".into(),
        ));
    };
    let (status, body) = match classified.key.operation {
        Operation::ListModels => (
            StatusCode::OK,
            super::model_catalogue::render_list(family, control.exposed_models()),
        ),
        Operation::GetModel => {
            let found = classified.model.as_ref().and_then(|id| {
                control
                    .exposed_models()
                    .into_iter()
                    .find(|model| &model.id == id)
            });
            match found {
                Some(model) => (
                    StatusCode::OK,
                    super::model_catalogue::render_model(family, &model),
                ),
                None => (
                    StatusCode::NOT_FOUND,
                    json!({ "error": { "message": "model not found" } }),
                ),
            }
        }
        Operation::CountTokens => {
            let count = core
                .host
                .count_tokens(
                    &target.upstream_model,
                    &request.body,
                    target.provider.settings.get("tokenizer_map"),
                )
                .await?;
            (StatusCode::OK, render_count(family, count))
        }
        _ => {
            return Err(CoreError::Internal(
                "non-local operation reached local serving".into(),
            ));
        }
    };
    let body = Bytes::from(serde_json::to_vec(&body).expect("local JSON serializes"));
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    let disposition = if status.is_success() {
        Disposition::Success
    } else {
        Disposition::Terminal
    };
    let funnel = FunnelCtx {
        request_id: request.request_id.clone(),
        target,
        credential_version: None,
        source_key: Some(classified.key),
        key: Some(classified.key),
        source_framing: classified.framing,
        target_framing: classified.framing,
        settle: SettleMode::Free,
        pricing: None,
        started,
        upstream_url: None,
        request_method: None,
        request_body: request.body.clone(),
        request_headers: None,
        client_headers: request.headers.clone(),
        requested_model: classified.model.clone(),
        response_headers: Some(headers.clone()),
        dedupe_key: None,
        owner_user_id: None,
        resource: None,
        admitted: true,
        surface_label: None,
    };
    Ok(funnel::local_buffered(
        core.host.as_ref(),
        funnel,
        status,
        headers,
        body,
        disposition,
    )
    .await)
}

fn render_count(family: WireFamily, count: u64) -> Value {
    match family {
        WireFamily::OpenAi => {
            json!({ "object": "response.input_tokens", "input_tokens": count })
        }
        WireFamily::Claude => json!({ "input_tokens": count }),
        WireFamily::Gemini => json!({ "totalTokens": count }),
    }
}
