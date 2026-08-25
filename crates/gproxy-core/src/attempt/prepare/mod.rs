use std::time::Instant;

use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx};
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

pub(crate) fn support<H: Host>(
    core: &Core<H>,
    target: &Target,
    key: OperationKey,
) -> Result<Option<ChannelSupport>, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    Ok(channel
        .descriptor()
        .supports
        .iter()
        .find(|support| support.source == key)
        .copied())
}

pub(crate) fn native_support<H: Host>(
    core: &Core<H>,
    target: &Target,
    key: OperationKey,
) -> Result<Option<ChannelSupport>, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    Ok(channel
        .descriptor()
        .supports
        .iter()
        .find(|support| support.source == key && support.target == key)
        .copied())
}

pub(crate) async fn prepare<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    target: &Target,
    ctx: &RequestCtx,
    classified: &Classified,
    admission: AdmissionCtx,
    started: Instant,
) -> Result<Prepared, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    support(core, target, classified.key)?.ok_or(CoreError::Unsupported)?;
    let credential = crate::execution::credential::load_fresh(
        core.host.as_ref(),
        channel,
        target.credential,
        &target.provider,
    )
    .await?;
    let support = channel
        .select_support(classified.key, &credential.secret)
        .filter(|selected| channel.descriptor().supports.contains(selected))
        .ok_or(CoreError::Unsupported)?;
    if !admission.admitted && support.source != support.target {
        return Err(CoreError::Unsupported);
    }
    let stream = classified.stream
        || support.target.operation == gproxy_protocol::Operation::StreamGenerateContent;
    let mut method = ctx.method.clone();
    let mut path = ctx.path.clone();
    let mut body = ctx.body.clone();
    if support.source != support.target {
        body = gproxy_transform::request(
            support.source,
            support.target,
            body,
            &target.upstream_model,
            stream,
        )
        .map_err(|error| CoreError::Transform(error.to_string()))?;
        (method, path) = gproxy_protocol::request_target(support.target, &target.upstream_model)
            .ok_or_else(|| {
                CoreError::Transform(format!("no request target for {:?}", support.target))
            })?;
    }
    let context = || PrepareCtx {
        key: support.target,
        stream,
        method: &method,
        path: &path,
        query: ctx.query.as_deref(),
        headers: &ctx.headers,
        body: &body,
        upstream_model: &target.upstream_model,
        provider_settings: &target.provider.settings,
        secret: &credential.secret,
    };
    let driver = health::result(
        core,
        target,
        credential.version,
        channel.operation_driver(context()),
    )
    .await?;
    let target_framing = gproxy_protocol::default_framing(support.target.kind, false);
    let facts = FunnelCtx {
        request_id: ctx.request_id.clone(),
        target: target.clone(),
        credential_version: Some(credential.version),
        source_key: Some(support.source),
        key: Some(support.target),
        source_framing: classified.framing,
        target_framing,
        settle: support.target.operation.spec().settle,
        pricing: control.pricing(&target.provider, &target.upstream_model),
        started,
        upstream_url: None,
        request_body: body.clone(),
        dedupe_key: classified.dedupe_key(target.provider.id),
        owner_user_id: admission.owner_user_id,
        resource: classified
            .resource()
            .map(|(kind, id)| (kind, id.to_owned())),
        admitted: admission.admitted,
        surface_label: None,
    };
    if let Some(driver) = driver {
        driver::validate(core, channel, target, admission, driver.as_ref())?;
        return Ok(Prepared {
            channel: channel.descriptor().id,
            stream: true,
            downstream_stream: classified.stream,
            facts,
            egress: Egress::Orchestrated(driver),
        });
    }
    let mut prepared =
        health::result(core, target, credential.version, channel.prepare(context())).await?;
    if prepared.websocket {
        return Err(CoreError::Unsupported);
    }
    crate::fingerprint::apply_prepared(&mut prepared, &target.provider)?;
    let target_framing = prepared
        .framing
        .unwrap_or_else(|| gproxy_protocol::default_framing(support.target.kind, false));
    let mut facts = facts;
    facts.target_framing = target_framing;
    facts.upstream_url = Some(prepared.request.uri().to_string());
    facts.request_body = prepared.request.body().clone();
    Ok(Prepared {
        channel: channel.descriptor().id,
        stream,
        downstream_stream: classified.stream,
        facts,
        egress: Egress::Http(Box::new(prepared.request)),
    })
}

fn channel<'a, H: Host>(core: &'a Core<H>, id: &str) -> Result<&'a dyn Channel, CoreError> {
    core.channels
        .get(id)
        .ok_or_else(|| CoreError::Internal(format!("unknown channel `{id}`")))
}
