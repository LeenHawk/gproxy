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
    control: &dyn ControlPlane,
    request: &RequestCtx,
    plan: &Plan,
    classified: &Classified,
    identity: &gproxy_channel_api::CallerIdentity,
    started: Instant,
) -> Option<Result<ExecOutcome, CoreError>> {
    let operation = classified.key.operation();
    let catalogue = matches!(operation, Operation::ListModels | Operation::GetModel);
    if !catalogue && !local_route(core, plan, classified.key) {
        return None;
    }
    Some(serve(core, control, request, plan, classified, identity, started).await)
}

fn local_route<H: Host>(core: &Core<H>, plan: &Plan, key: gproxy_protocol::OperationKey) -> bool {
    plan.targets.iter().any(|target| {
        let Some(channel) = core.channels.get(&target.provider.channel) else {
            return false;
        };
        super::local_models::route_is_local(channel, target, key)
    })
}

async fn serve<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    request: &RequestCtx,
    plan: &Plan,
    classified: &Classified,
    identity: &gproxy_channel_api::CallerIdentity,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let target = plan
        .targets
        .first()
        .cloned()
        .ok_or(CoreError::NoCredentials)?;
    let OperationKind::Family(family) = classified.key.kind() else {
        return Err(CoreError::Internal(
            "local operation has a non-family wire kind".into(),
        ));
    };
    let (status, body) = match classified.key.operation() {
        Operation::ListModels => {
            let scoped = matches!(&request.mode, crate::boundary::RoutingMode::Scoped { .. });
            let mut models = if scoped {
                Vec::new()
            } else {
                control.exposed_models()
            };
            if !scoped {
                models.extend(control.provider_catalogue());
            }
            models.extend(super::model_refresh::run(core, control, request, plan, identity).await);
            models.sort_by(|left, right| left.id.cmp(&right.id));
            models.dedup_by(|left, right| left.id == right.id);
            models.retain(|model| {
                control.catalogue_visible(identity, Some(&model.id), &request.mode)
            });
            (
                StatusCode::OK,
                super::model_catalogue::render_list(family, models),
            )
        }
        Operation::GetModel => {
            let models = if matches!(&request.mode, crate::boundary::RoutingMode::Scoped { .. }) {
                super::model_refresh::for_local_get(
                    core, control, request, plan, classified, identity,
                )
                .await
            } else {
                control.exposed_models()
            };
            let found = classified
                .model
                .as_ref()
                .and_then(|id| models.into_iter().find(|model| &model.id == id))
                .filter(|model| {
                    control.catalogue_visible(identity, Some(&model.id), &request.mode)
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
        upstream_started_at_ms: None,
        request_id: request.request_id.clone(),
        target,
        credential_version: None,
        source_key: Some(classified.key),
        key: Some(classified.key),
        source_framing: classified.framing,
        target_framing: classified.framing,
        settle: SettleMode::Free,
        pricing: None,
        pricing_control: None,
        usage_channel: None,
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
        traffic_policy: None,
        traffic_blacklist: None,
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
        WireFamily::OpenAi => json!({ "object": "response.input_tokens", "input_tokens": count }),
        WireFamily::Claude => json!({ "input_tokens": count }),
        WireFamily::Gemini => json!({ "totalTokens": count }),
    }
}
