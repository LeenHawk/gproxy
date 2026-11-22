//! A Responses WebSocket client whose upstream speaks Chat, Claude or Gemini
//! over HTTP: core fabricates the accepted socket, decodes each
//! `response.create` message, converts it as one HTTP stream and writes the
//! Responses server events back as text frames on the same lane. The other
//! direction, an HTTP client over a Responses WebSocket upstream, is
//! `gproxy_convert::responses_ws`.
//!
//! HTTP segments reuse the admitted retry plan for explicit rejections. Once
//! accepted, the stream is never replayed after a transport or event failure.

mod bridge;

use crate::{
    Core, CoreError, CoreResult, ProtocolState, RequestContext, StateScope, UsageState,
    WebSocketExecution,
    api::{Execution, UsageCompletion, lifecycle::now_ms},
    execute::Funnel,
};
use gproxy_convert::{
    STATE_TTL,
    generate::{OwnedState, stream_settings},
    responses_ws::FRAME_QUEUE,
};
use gproxy_protocol::{
    Dialect, WireRequest, WireResponse,
    adapt::generate::stream::StreamSettings,
    capability::UpstreamConnection,
    connection::{TransportError, WebSocket, WsFrame},
    transform::{TransformError, identity::IdentityTarget},
    wire::openai::responses::websocket::{
        ClientEvent, ErrorDetail, ErrorMessage, ErrorMessageType,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, StatusCode};
use std::{collections::HashSet, sync::Arc, time::SystemTime};
use tokio::sync::mpsc;

fn error_frame(status: u16, lane: Option<String>, message: String) -> WsFrame {
    let message = ErrorMessage {
        type_: ErrorMessageType::Error,
        status: Some(status),
        stream_id: lane,
        error: ErrorDetail {
            type_: Some("invalid_request_error".into()),
            code: None,
            message,
            param: None,
        },
    };
    WsFrame::Text(serde_json::to_string(&message).unwrap_or_default())
}

fn closed() -> TransportError {
    Box::new(std::io::Error::other("client connection closed"))
}

type Outgoing = mpsc::Sender<Result<WsFrame, TransportError>>;

/// Responses WebSocket client over an HTTP upstream speaking `target`.
/// The session owns turn admission; HTTP segments reuse its admitted executor.
pub(crate) async fn serve<C: BatchConnectionTrait + Send + Sync + 'static>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    wire: &WireRequest<()>,
    funnel: Arc<Funnel>,
    completion: UsageCompletion,
    target: Dialect,
    session: Option<crate::ResponsesSession>,
) -> CoreResult<WebSocketExecution> {
    let session = session.ok_or_else(|| {
        CoreError::InvalidTarget(
            "Responses HTTP bridging requires connect_responses_session".into(),
        )
    })?;
    if !matches!(
        target,
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::Claude | Dialect::Gemini
    ) {
        funnel.finish(UsageState::Failed).await;
        return Err(CoreError::Transform(TransformError::unsupported(
            "responses.websocket",
            format!("no generation from a Responses WebSocket client to {target:?}"),
        )));
    }
    let now = now_ms();
    let selection = match core
        .select_credential(&request, &wire.headers, &HashSet::new(), now)
        .await
    {
        Ok(selection) => selection,
        Err(error) => {
            funnel.finish(UsageState::Failed).await;
            return Err(error);
        }
    };
    let limits = request.execution_limits();
    let provider = request.target.provider.clone();
    let credential = selection.credential;
    session.bind(
        request.clone(),
        credential.clone(),
        target,
        None,
        selection.assignment.as_ref().map(|h| h.reference.clone()),
    );
    // The socket is fabricated locally; binding the session is the whole
    // preparation, so a reservation is active as soon as it exists.
    if let Some(handle) = &selection.assignment {
        core.settle_assignment(handle, crate::session::AssignmentOutcome::Activated, now)
            .await?;
    }
    let capability = limits.capability(None);
    let Some(model) = request.target.upstream_model.clone() else {
        funnel.finish(UsageState::Failed).await;
        return Err(CoreError::Transform(TransformError::missing_metadata(
            "upstream_model",
        )));
    };
    let identity_target = IdentityTarget::new(&model, target)
        .and_then(|t| t.with_origin(&provider.entity.id))
        .map_err(|e| {
            CoreError::Transform(TransformError::shape("identity.target", e.to_string()))
        })?;
    let scope = StateScope {
        scope: request.scope.clone(),
        provider_id: provider.entity.id.clone(),
        conversation: request
            .session
            .as_ref()
            .filter(|s| s.is_stable())
            .map(|s| s.id.clone()),
    };
    let conversation_key = scope
        .conversation
        .clone()
        .unwrap_or_else(|| request.request_id.clone());
    let mut owned = OwnedState {
        store: ProtocolState::new(core, capability),
        scope,
        target: identity_target,
        conversation_key,
        // Millisecond precision: the state store persists expiry as ms and the
        // identity records written under it are compared against the read-back value.
        expires_at: SystemTime::UNIX_EPOCH
            + std::time::Duration::from_millis(now.max(0) as u64)
            + STATE_TTL,
    };
    if let Some(binding) = session.binding() {
        owned.expires_at = SystemTime::UNIX_EPOCH
            + std::time::Duration::from_millis(binding.expires_at_ms.max(0) as u64);
    }
    let codec = limits.codec();
    let settings = stream_settings(codec, Dialect::OpenAi, None);
    Ok(bridge::serve(
        bridge::Config {
            owned,
            settings,
            headers: wire.headers.clone(),
            cancellation: request.cancellation.clone(),
        },
        session,
        funnel,
        completion,
    ))
}
