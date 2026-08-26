use web_time::Instant;

use gproxy_channel_api::SurfaceRequest;
use gproxy_protocol::SettleMode;

use crate::api::Core;
use crate::boundary::ExecOutcome;
use crate::control::{Pricing, Target};
use crate::error::CoreError;
use crate::funnel::error as funnel_error;
use crate::funnel::{self, FunnelCtx};
use crate::host::{Host, UpstreamTransport};

pub(crate) async fn request<H: Host>(
    core: &Core<H>,
    target: &Target,
    request: SurfaceRequest,
    websocket: bool,
    request_id: String,
    started: Instant,
    pricing: Option<Pricing>,
) -> Result<ExecOutcome, CoreError> {
    let channel = core.channels.get(&target.provider.channel).ok_or_else(|| {
        CoreError::Internal(format!(
            "provider references unknown channel `{}`",
            target.provider.channel
        ))
    })?;
    if let Some(key) = request.key
        && (!channel
            .descriptor()
            .supports
            .iter()
            .any(|support| support.target == key)
            || matches!(
                key.operation.spec().settle,
                SettleMode::OnCompletedStatus | SettleMode::OnSessionEnd
            ))
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
    let mut prepared = match channel.prepare_surface(
        &request,
        websocket,
        &target.provider.settings,
        &credential.secret,
    ) {
        Ok(prepared) => prepared,
        Err(error @ gproxy_channel_api::ChannelError::Secret(_)) => {
            crate::funnel::health::dead(
                core.host.as_ref(),
                target,
                Some(credential.version),
                "credential rejected during surface preparation",
            )
            .await;
            return Err(error.into());
        }
        Err(error @ gproxy_channel_api::ChannelError::Refresh(_)) => {
            crate::funnel::health::degraded(
                core.host.as_ref(),
                target,
                Some(credential.version),
                None,
                "surface preparation refresh failed",
            )
            .await;
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };
    if prepared.websocket != websocket {
        return Err(CoreError::Internal(
            "surface websocket preparation disagrees with its table action".into(),
        ));
    }
    if websocket && request.key.is_some() {
        return Err(CoreError::Unsupported);
    }
    crate::fingerprint::apply_prepared(&mut prepared, &target.provider)?;
    let source_framing = request
        .key
        .map_or(gproxy_protocol::StreamFraming::Sse, |key| {
            gproxy_protocol::default_framing(key.kind, false)
        });
    let target_framing = prepared.framing.unwrap_or(source_framing);
    let facts = FunnelCtx {
        request_id,
        target: target.clone(),
        credential_version: Some(credential.version),
        source_key: request.key,
        key: request.key,
        source_framing,
        target_framing,
        settle: request
            .key
            .map(|key| key.operation.spec().settle)
            .unwrap_or(SettleMode::Free),
        pricing,
        started,
        upstream_url: Some(prepared.request.uri().to_string()),
        request_body: prepared.request.body().clone(),
        request_headers: None,
        dedupe_key: None,
        owner_user_id: None,
        resource: None,
        admitted: true,
        surface_label: Some(request.label),
    };
    if websocket {
        return match core.host.transport().open_websocket(prepared.request).await {
            Ok(socket) => {
                core.host
                    .record_credential_health(
                        facts.target.credential,
                        facts
                            .credential_version
                            .expect("surface credential version is loaded"),
                        crate::CredentialHealth::Healthy,
                        None,
                        "upstream websocket connected",
                    )
                    .await;
                Ok(funnel::websocket(core.host.clone(), facts, socket))
            }
            Err(error) => {
                crate::funnel::health::degraded(
                    core.host.as_ref(),
                    &facts.target,
                    facts.credential_version,
                    None,
                    "upstream websocket failed",
                )
                .await;
                funnel_error::attempt_transport(core.host.as_ref(), &facts, &error).await;
                Err(error.into())
            }
        };
    }

    let response = match core.host.transport().send(prepared.request).await {
        Ok(response) => response,
        Err(error) => {
            crate::funnel::health::degraded(
                core.host.as_ref(),
                &facts.target,
                facts.credential_version,
                None,
                "upstream transport failed",
            )
            .await;
            funnel_error::attempt_transport(core.host.as_ref(), &facts, &error).await;
            return Err(error.into());
        }
    };
    super::response::relay(core, channel, request.stream, request.key, facts, response).await
}
