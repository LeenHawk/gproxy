use web_time::Instant;

use gproxy_channel_api::{Channel, ChannelError, ChannelRouteAction, ChannelSupport, PrepareCtx};
use gproxy_protocol::OperationKey;

use super::{AdmissionCtx, Egress, Prepared};
use crate::api::Core;
use crate::boundary::RequestCtx;
use crate::control::{ControlPlane, Target};
use crate::error::CoreError;
use crate::execution::request::Classified;
use crate::funnel::FunnelCtx;
use crate::host::Host;

mod driver;
mod health;
#[cfg(test)]
mod tests;

pub(crate) fn support<H: Host>(
    core: &Core<H>,
    target: &Target,
    key: OperationKey,
) -> Result<Option<ChannelSupport>, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    let selected = declared_support(channel, key);
    Ok(selected.and_then(|support| route_support(target, support)))
}

pub(crate) fn native_support<H: Host>(
    core: &Core<H>,
    target: &Target,
    key: OperationKey,
) -> Result<Option<ChannelSupport>, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    Ok(declared_support(channel, key).filter(|support| support.target == key))
}

pub(crate) async fn prepare<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    target: &Target,
    ctx: &RequestCtx,
    classified: &Classified,
    admission: AdmissionCtx,
    started: Instant,
) -> Result<Prepared, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    let (pin_key, pinned_model) = super::refusal::pin::lookup(
        core,
        channel,
        target,
        classified.session_id(admission.owner_user_id).as_deref(),
        &ctx.body,
    )
    .await;
    let mut pinned_target = target.clone();
    let pinned = pinned_model.is_some();
    if let Some(model) = pinned_model {
        pinned_target.upstream_model = model;
        core.host
            .admit_retry(
                &ctx.request_id,
                &pinned_target,
                &ctx.body,
                classified.key.operation().spec().settle,
            )
            .await?;
    }
    let target = &pinned_target;
    support(core, target, classified.key)?.ok_or(CoreError::Unsupported)?;
    if classified.key.operation().spec().settle == gproxy_protocol::SettleMode::OnSessionEnd
        && core.host.spawner().is_none()
    {
        return Err(CoreError::Unsupported);
    }
    let credential = crate::execution::credential::load_fresh(
        core.host.as_ref(),
        channel,
        target.credential,
        &target.provider,
    )
    .await?;
    let support = channel
        .select_support(classified.key, &credential.secret)
        .filter(|selected| executable(channel, selected))
        .and_then(|support| route_support(target, support))
        .ok_or(CoreError::Unsupported)?;
    if !admission.admitted && support.source != support.target {
        return Err(CoreError::Unsupported);
    }
    let stream = upstream_stream(classified.stream, support.source, support.target);
    let mut method = ctx.method.clone();
    let mut path = ctx.path.clone();
    let mut query = ctx.query.clone();
    let mut body = ctx.body.clone();
    let mut normalized_media = false;
    if support.source != support.target {
        (body, normalized_media) = super::media::normalize(&ctx.headers, body)?;
        body = gproxy_transform::request(
            support.source,
            support.target,
            body,
            &target.upstream_model,
            stream,
        )
        .map_err(|error| CoreError::Transform(error.to_string()))?;
        let target_parameter = classified
            .resource()
            .map(|(_, id)| id)
            .unwrap_or(&target.upstream_model);
        (method, path) = gproxy_protocol::request_target(support.target, target_parameter)
            .ok_or_else(|| {
                CoreError::Transform(format!("no request target for {:?}", support.target))
            })?;
        query =
            gproxy_transform::request_query(support.source, support.target, ctx.query.as_deref())
                .map_err(|error| CoreError::Transform(error.to_string()))?;
    }
    let mutation = crate::process::apply_request(
        &target.rules.process,
        support.target,
        crate::process::RuleModels::new(
            &target.upstream_model,
            classified
                .model
                .as_deref()
                .filter(|model| *model != target.upstream_model),
        ),
        &ctx.headers,
        body,
    );
    body = mutation.body;
    if stream != classified.stream {
        body = align_stream_flag(body, support.target, stream);
    }
    // After the rules, not before: a rule that inserts text can carry a magic marker,
    // and v2 shaped at this point for exactly that reason. The provider switch stays
    // authoritative so operators can opt into this client-to-proxy protocol.
    if support.target.kind()
        == gproxy_protocol::OperationKind::ContentGeneration(
            gproxy_protocol::ContentGenerationKind::ClaudeMessages,
        )
        && target
            .provider
            .settings
            .get("enable_claude_magic_cache")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        body = gproxy_channels::apply_claude_magic_cache(body)?;
    }
    let traffic_policy = channel
        .descriptor()
        .traffic_policy
        .effective_traffic_policy(&target.provider.settings)
        .map_err(|error| CoreError::Channel(ChannelError::Prepare(error)))?;
    let source_headers = mutation.headers.as_ref().unwrap_or(&ctx.headers);
    let mut request_headers = crate::execution::forwarding::request_headers(
        source_headers,
        &traffic_policy,
        &target.provider.traffic_blacklist,
    );
    if normalized_media {
        request_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        request_headers.remove(http::header::CONTENT_LENGTH);
    }
    if support.source != support.target {
        body = super::media::restore(support.target, &mut request_headers, body)?;
    }
    let request_query = crate::execution::forwarding::request_query(
        query.as_deref(),
        &traffic_policy,
        &target.provider.traffic_blacklist,
    );
    let session_id = classified.session_id(admission.owner_user_id);
    let context = PrepareCtx {
        key: support.target,
        session_id: session_id.as_deref(),
        stream,
        method: &method,
        path: &path,
        query: request_query.as_deref(),
        headers: &request_headers,
        body: &body,
        upstream_model: &target.upstream_model,
        provider_settings: &target.provider.settings,
        secret: &credential.secret,
    };
    let mut refusal = super::refusal::Replay::read(channel, &context)?;
    if let Some(replay) = refusal.as_mut() {
        replay.pin_key = pin_key;
        replay.pinned = pinned;
    }
    let mut fallback_headers = request_headers.clone();
    let fallback_body = if let Some(replay) = refusal.as_ref() {
        replay.prepare_headers(&mut fallback_headers);
        replay.prepare_body(&body)
    } else {
        body.clone()
    };
    let mut channel_settings = target.provider.settings.clone();
    if pinned {
        channel_settings["claude_fallback_mode"] = serde_json::json!("off");
    }
    let context = PrepareCtx {
        body: &fallback_body,
        headers: &fallback_headers,
        provider_settings: &channel_settings,
        ..context
    };
    let driver = health::result(
        core,
        target,
        credential.version,
        channel.operation_driver(context),
    )
    .await?;
    let target_framing = gproxy_protocol::default_framing(support.target.kind(), false);
    let facts = FunnelCtx {
        pricing_control: Some(std::sync::Arc::from(control.detached())),
        usage_channel: core.channels.shared(channel.descriptor().id),
        upstream_started_at_ms: None,
        request_id: ctx.request_id.clone(),
        target: target.clone(),
        credential_version: Some(credential.version),
        source_key: Some(support.source),
        key: Some(support.target),
        source_framing: classified.framing,
        target_framing,
        settle: support.target.operation().spec().settle,
        pricing: control
            .pricing(&target.provider, &target.upstream_model)
            .map(|pricing| pricing.for_request(&ctx.body)),
        started,
        upstream_url: None,
        request_method: None,
        request_body: body.clone(),
        request_headers: (support.target.operation().spec().settle
            == gproxy_protocol::SettleMode::OnSessionEnd)
            .then(|| request_headers.clone()),
        client_headers: ctx.headers.clone(),
        requested_model: classified.model.clone(),
        response_headers: None,
        dedupe_key: classified.dedupe_key(target.provider.id),
        owner_user_id: admission.owner_user_id,
        resource: classified
            .resource()
            .map(|(kind, id)| (kind, id.to_owned())),
        admitted: admission.admitted,
        surface_label: None,
        traffic_policy: Some(traffic_policy),
        traffic_blacklist: Some(target.provider.traffic_blacklist.clone()),
    };
    if let Some(driver) = driver {
        driver::validate(core, channel, target, admission, driver.as_ref())?;
        return Ok(Prepared {
            quota_accounted: channel.quota_capabilities(&credential.secret).is_some()
                && facts.settle != gproxy_protocol::SettleMode::Free,
            channel: channel.descriptor().id,
            stream: true,
            downstream_stream: classified.stream,
            facts,
            egress: Egress::Orchestrated(driver),
            refusal: None,
        });
    }
    let mut prepared =
        health::result(core, target, credential.version, channel.prepare(context)).await?;
    crate::fingerprint::apply_prepared(&mut prepared, &target.provider)?;
    let target_framing = prepared
        .framing
        .unwrap_or_else(|| gproxy_protocol::default_framing(support.target.kind(), false));
    let mut facts = facts;
    facts.target_framing = target_framing;
    facts.upstream_url = Some(prepared.request.uri().to_string());
    facts.request_method = Some(prepared.request.method().clone());
    if let Some(replay) = refusal.as_mut() {
        replay.capture(&prepared.request);
    }
    facts.request_body = prepared.request.body().clone();
    facts.request_headers = Some(prepared.request.headers().clone());
    Ok(Prepared {
        quota_accounted: channel.quota_capabilities(&credential.secret).is_some()
            && facts.settle != gproxy_protocol::SettleMode::Free,
        channel: channel.descriptor().id,
        stream,
        downstream_stream: classified.stream,
        refusal,
        facts,
        egress: if prepared.websocket {
            Egress::WebSocket(Box::new(prepared.request))
        } else {
            Egress::Http(Box::new(prepared.request))
        },
    })
}

/// Stream-ness follows the routed target, not the client. A route onto the
/// streaming sibling forces an event stream the funnel collapses for a
/// non-stream client; a route onto the buffered sibling fetches one object the
/// funnel synthesizes into the client's stream.
fn upstream_stream(client_stream: bool, source: OperationKey, target: OperationKey) -> bool {
    use gproxy_protocol::Operation::{GenerateContent, StreamGenerateContent};
    match (source.operation(), target.operation()) {
        (StreamGenerateContent, GenerateContent) => false,
        (_, StreamGenerateContent) => true,
        _ => client_stream,
    }
}

/// A same-kind sibling route leaves the client's body untouched, so its wire
/// flag still says what the client asked for; the target decides.
fn align_stream_flag(body: bytes::Bytes, target: OperationKey, stream: bool) -> bytes::Bytes {
    use gproxy_protocol::ContentGenerationKind::{
        ClaudeMessages, OpenAiChat, OpenAiResponses, OpenAiResponsesWebSocket,
    };
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };
    let Some(object) = value.as_object_mut() else {
        return body;
    };
    let flagged = matches!(
        target.kind(),
        gproxy_protocol::OperationKind::ContentGeneration(
            ClaudeMessages | OpenAiChat | OpenAiResponses | OpenAiResponsesWebSocket
        )
    );
    if stream && flagged {
        object.insert("stream".into(), serde_json::Value::Bool(true));
    } else if !stream {
        object.remove("stream");
    } else {
        return body;
    }
    bytes::Bytes::from(serde_json::to_vec(&value).expect("JSON serializes"))
}

pub(crate) fn executable(channel: &dyn Channel, selected: &ChannelSupport) -> bool {
    channel.descriptor().supports.contains(selected)
        || (selected.action == ChannelRouteAction::TransformTo
            && channel.descriptor().supports.iter().any(|support| {
                support.source == selected.target
                    && support.target == selected.target
                    && support.action == ChannelRouteAction::Passthrough
            }))
}

fn route_support(target: &Target, support: ChannelSupport) -> Option<ChannelSupport> {
    if matches!(
        support.action,
        ChannelRouteAction::Local | ChannelRouteAction::Unsupported
    ) {
        return None;
    }
    match crate::routing::decide(&target.rules.routing, support.source) {
        None => Some(support),
        Some(crate::routing::RoutingDecision::Passthrough) => Some(ChannelSupport {
            source: support.source,
            target: support.source,
            action: ChannelRouteAction::Passthrough,
        }),
        Some(crate::routing::RoutingDecision::TransformTo(destination)) => Some(ChannelSupport {
            source: support.source,
            target: destination,
            action: ChannelRouteAction::TransformTo,
        }),
        Some(crate::routing::RoutingDecision::Local)
        | Some(crate::routing::RoutingDecision::Unsupported) => None,
    }
}

fn declared_support(channel: &dyn Channel, source: OperationKey) -> Option<ChannelSupport> {
    if let Some(route) = channel
        .routing_table()
        .iter()
        .find(|support| support.source == source)
    {
        return matches!(
            route.action,
            ChannelRouteAction::Passthrough | ChannelRouteAction::TransformTo
        )
        .then_some(*route);
    }
    channel
        .descriptor()
        .supports
        .iter()
        .find(|support| support.source == source)
        .copied()
}

fn channel<'a, H: Host>(core: &'a Core<H>, id: &str) -> Result<&'a dyn Channel, CoreError> {
    core.channels
        .get(id)
        .ok_or_else(|| CoreError::Internal(format!("unknown channel `{id}`")))
}
