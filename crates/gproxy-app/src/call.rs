//! The data plane: one decoded client request, admitted, then executed.
//!
//! This is the whole of the bridge between this crate and the engine. A host
//! decodes a transport into a [`DataPlaneRequest`], authentication has already
//! produced a [`Caller`], and [`App::call`] does the rest: admit, hand the
//! engine exactly what admission decided, and give back the execution together
//! with the charges the request is holding.
//!
//! # Nothing is decided twice
//!
//! The builder is driven with every value out of the [`Admitted`] — scope,
//! attribution, budgets, the provider set, the credential set, the session —
//! and with nothing that was not decided there. The engine never sees the
//! `Caller`, so it cannot re-derive an answer this layer already gave, and
//! this layer never guesses at routing, which is the engine's.
//!
//! The one value that is read here *and* there is the model name. It is parsed
//! out of the JSON body (the sdk's own helper for that is private, so the same
//! two lines exist in both places) and then passed to the builder explicitly,
//! so the name permissions and rate limits were decided against is the name
//! resolution uses. A dialect that puts the model in the path instead — the
//! Gemini shape — has already been parsed by the host, which sets
//! [`DataPlaneRequest::model`].
//!
//! # The leases outlive this call
//!
//! [`CallOutcome::admitted`] carries the rate-limit charges. A concurrency
//! permit measures requests *in flight*, and a streamed response is still in
//! flight long after `call()` has returned, so the host must keep the
//! `Admitted` alive for as long as it is writing the response — dropping it
//! early hands the slot back to the next request while this one is still
//! using it. `Admitted::finish()` has already been called: window counters
//! stand, permits are returned when the value is finally dropped. A host that
//! can await should end the request with [`Admitted::release`] instead of
//! relying on the drop path, which has to spawn.
//!
//! # So does the capture
//!
//! [`CallOutcome::capture`] is the request's own `downstream_records` row, and it
//! is unwritten when `call()` returns for the same reason: the response has
//! not been sent yet. The host feeds it the chunks it writes and then settles
//! it, which is also what writes the edges to the upstream attempts. See
//! [`crate::capture`]; `None` means `enable_downstream_log` is off and there
//! is nothing to feed.

use std::{collections::BTreeSet, time::Duration};

use gproxy_core::{HttpExecution, WebSocketExecution};
use gproxy_protocol::{HttpBody, OperationKey, WireRequest, connection::Bytes};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    AdmissionRequest, Admitted, App, AppError, Caller,
    admission::strip_gateway_header,
    capture::{CaptureOutcome, DownstreamCapture},
};

/// One client request, decoded far enough for this crate to decide on it.
///
/// `parts` and `body` are the request as received: the path, the query and
/// every header a client sent are forwarded to the upstream, because a channel
/// may need any of them. The gateway's own session header is the single
/// exception and is removed before forwarding.
pub struct DataPlaneRequest {
    /// The id this request is observed, logged and settled under. A host that
    /// already minted one for its own logs should pass that one, so the two
    /// trails join.
    pub request_id: String,
    pub operation: OperationKey,
    pub parts: http::request::Parts,
    /// The buffered body. Decoding (gzip, zstd) belongs to the host; by the
    /// time it reaches here it is the bytes a channel will forward.
    pub body: Bytes,
    /// The client address the host resolved, for the operator's log. Nothing
    /// here decides on it: a rate limit is a row about a key or a person, and
    /// an address a proxy can rewrite must not select one.
    pub client_ip: Option<String>,
    /// Restrict resolution to the provider selected by the ingress mount.
    pub provider_id: Option<String>,
    /// The `agent_sessions` row the host resolved for this request, when it is
    /// a long-lived agent session whose credential binding core manages as an
    /// assignment rather than as affinity.
    pub agent_session_id: Option<String>,
    /// The model, when the dialect names it somewhere only the host can see —
    /// the Gemini path shape, `/v1beta/models/{model}:generateContent`. Left
    /// `None` the body's `model` field is used.
    pub model: Option<String>,
    /// Wall clock for the whole call, across every provider it tries.
    pub deadline: Option<Duration>,
    /// Cancelled when the client goes away, so an abandoned stream stops
    /// costing money upstream.
    pub cancellation: Option<CancellationToken>,
}

impl DataPlaneRequest {
    /// The required parts: no provider restriction, no agent session, no
    /// deadline and no cancellation.
    pub fn new(
        request_id: impl Into<String>,
        operation: OperationKey,
        parts: http::request::Parts,
        body: Bytes,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            operation,
            parts,
            body,
            client_ip: None,
            provider_id: None,
            agent_session_id: None,
            model: None,
            deadline: None,
            cancellation: None,
        }
    }
}

impl std::fmt::Debug for DataPlaneRequest {
    /// Shape and size only. The headers carry the caller's credential and the
    /// body carries their prompt; neither belongs in a log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataPlaneRequest")
            .field("request_id", &self.request_id)
            .field("operation", &self.operation)
            .field("path", &self.parts.uri.path())
            .field("body_bytes", &self.body.len())
            .field("provider_id", &self.provider_id)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

/// A started HTTP call and the admission decision behind it.
///
/// **Keep `admitted` alive for as long as the response is being written.** The
/// module documentation says why: a concurrency permit is released when this
/// value drops, and a stream that is still running is still in flight.
pub struct CallOutcome {
    pub execution: HttpExecution,
    pub admitted: Admitted,
    /// The request's downstream capture, with the response head already
    /// recorded. `None` when `enable_downstream_log` is off. Feed it the
    /// response chunks as they are written and settle it at the end.
    pub capture: Option<DownstreamCapture>,
}

impl std::fmt::Debug for CallOutcome {
    /// The status and the decision, not the body: an execution's body is the
    /// caller's content and an error message is not where it belongs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallOutcome")
            .field("status", &self.execution.response().status)
            .field("scope", &self.admitted.scope)
            .field("leases", &self.admitted.rate_limit_leases.len())
            .field("capture", &self.capture.is_some())
            .finish()
    }
}

/// A completed websocket handshake and the admission decision behind it. Same
/// lifetime rule as [`CallOutcome`], and more so: a socket can stay open for
/// hours.
pub struct ConnectOutcome {
    pub execution: WebSocketExecution,
    pub admitted: Admitted,
    /// The downstream capture, with **no** response head recorded: a
    /// handshake has no `WireResponse`, so the host calls
    /// [`DownstreamCapture::record_response_head`] with `101` once it has
    /// accepted the upgrade, which is what marks the row a `WsConnection`.
    pub capture: Option<DownstreamCapture>,
}

impl std::fmt::Debug for ConnectOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let accepted = matches!(
            self.execution.response(),
            gproxy_protocol::capability::UpstreamConnection::Connected { .. }
        );
        f.debug_struct("ConnectOutcome")
            .field("accepted", &accepted)
            .field("scope", &self.admitted.scope)
            .field("leases", &self.admitted.rate_limit_leases.len())
            .field("capture", &self.capture.is_some())
            .finish()
    }
}

/// Drive one of the two builders with everything admission decided.
///
/// A macro rather than a function because `CallBuilder` and `ConnectBuilder`
/// are separate types with the same setters, and the point is that the two
/// entry points cannot forget different things.
macro_rules! drive {
    ($builder:expr, $admitted:expr, $request:expr, $model:expr) => {{
        let mut builder = $builder
            .scope($admitted.scope.clone())
            .attribution($admitted.attribution.clone())
            .budgets($admitted.budgets.clone())
            .providers($admitted.providers.clone())
            .credentials($admitted.credentials.clone())
            .request_id($request.request_id.clone());
        // The session carries its own `agent_session_id`, attached by the
        // ladder, so it is not set a second time here.
        if let Some(session) = $admitted.session.clone() {
            builder = builder.session(session);
        }
        if let Some(model) = $model {
            builder = builder.model(model);
        }
        if let Some(deadline) = $request.deadline {
            builder = builder.deadline(deadline);
        }
        if let Some(token) = &$request.cancellation {
            builder = builder.cancellation(token.clone());
        }
        builder
    }};
}
pub(crate) use drive;

impl<C: BatchConnectionTrait + Send + Sync + 'static> App<C> {
    /// Admit one request and execute it.
    ///
    /// Refuses with `Forbidden` before anything is sent when the caller may
    /// reach no provider, and with `RateLimited` when a configured limit
    /// rejected it. A failure after admission gives every charge back before
    /// it returns; a success has already called `Admitted::finish()`.
    pub async fn call(
        &self,
        caller: &Caller,
        request: DataPlaneRequest,
    ) -> Result<CallOutcome, AppError> {
        let json = json_body(&request.body);
        let model = request
            .model
            .clone()
            .or_else(|| body_model(json.as_ref()).map(str::to_owned));

        let data = self.data();
        let mut admitted = self
            .admit(&data, caller, &request, json.as_ref(), model.as_deref())
            .await?;

        // Opened before the engine is handed anything, and out of the request
        // as it arrived rather than out of the rewritten wire request: a log
        // is what the client sent, not what a channel made of it.
        let mut capture = DownstreamCapture::open(&data.observation, &request, caller, &admitted);

        let DataPlaneRequest {
            operation,
            parts,
            body,
            ..
        } = &request;
        let mut headers = parts.headers.clone();
        // Also stripped inside the handle; doing it here as well means the
        // header cannot reach an upstream through any other path this crate
        // grows later.
        strip_gateway_header(&mut headers);
        let wire = WireRequest {
            method: parts.method.clone(),
            path: parts.uri.path().to_owned(),
            query: parts.uri.query().map(str::to_owned),
            headers,
            body: HttpBody::Bytes(body.clone()),
        };

        let builder = drive!(
            self.gproxy().call(*operation, wire),
            admitted,
            request,
            model.as_deref()
        );
        match builder.send().await {
            Ok(execution) => {
                admitted.finish();
                // The head is known now and the host would otherwise have to
                // remember to copy it off the execution before consuming it.
                if let Some(capture) = capture.as_mut() {
                    let response = execution.response();
                    capture.record_response_head(response.status, &response.headers);
                }
                Ok(CallOutcome {
                    execution,
                    admitted,
                    capture,
                })
            }
            Err(error) => {
                // Nothing ran, so nothing should have been spent.
                admitted.release().await;
                // A request that reached admission and then failed is still a
                // request the operator wants in the log, and there is no
                // outcome to hand the capture back in. The result is dropped
                // on purpose: `finish` has already logged, and a logging
                // failure must not replace the failure that caused it.
                self.close_capture(capture, &error).await;
                Err(error.into())
            }
        }
    }

    /// Admit one websocket handshake and perform it. Same decisions, same
    /// lease rule; the body is not part of a handshake, so the session ladder
    /// sees headers only.
    pub async fn connect(
        &self,
        caller: &Caller,
        request: DataPlaneRequest,
    ) -> Result<ConnectOutcome, AppError> {
        let model = request.model.clone();

        let data = self.data();
        let mut admitted = self
            .admit(&data, caller, &request, None, model.as_deref())
            .await?;

        let capture = DownstreamCapture::open(&data.observation, &request, caller, &admitted);

        let DataPlaneRequest {
            operation, parts, ..
        } = &request;
        let mut headers = parts.headers.clone();
        strip_gateway_header(&mut headers);
        let wire = WireRequest {
            method: parts.method.clone(),
            path: parts.uri.path().to_owned(),
            query: parts.uri.query().map(str::to_owned),
            headers,
            body: (),
        };

        let builder = drive!(
            self.gproxy().connect(*operation, wire),
            admitted,
            request,
            model.as_deref()
        );
        match builder.send().await {
            Ok(execution) => {
                admitted.finish();
                Ok(ConnectOutcome {
                    execution,
                    admitted,
                    capture,
                })
            }
            Err(error) => {
                admitted.release().await;
                self.close_capture(capture, &error).await;
                Err(error.into())
            }
        }
    }

    /// Write the record of a request that never got a response.
    ///
    /// Separate from the success path because there is nobody to hand the
    /// capture to: the host is about to receive an error, not an outcome. The
    /// write cannot fail the request — it has already failed — so its result
    /// is discarded after `finish` has logged it.
    async fn close_capture(
        &self,
        capture: Option<DownstreamCapture>,
        error: &gproxy_sdk::SdkError,
    ) {
        if let Some(capture) = capture {
            let outcome = CaptureOutcome::Failed {
                error: error.to_string(),
            };
            let _ = capture.finish(self.gproxy().store(), outcome).await;
        }
    }

    /// The shared half: the instance's live provider and credential sets from
    /// the engine's snapshot, then admission.
    ///
    /// Both sets come from `CoreData` rather than from the database, because
    /// they must be what the engine can actually *use*: a credential that is
    /// disabled, retired or fully blocked is already gone from there, and
    /// admission narrows that set without ever adding to it.
    pub(crate) async fn admit(
        &self,
        data: &crate::AppData,
        caller: &Caller,
        request: &DataPlaneRequest,
        json: Option<&Value>,
        model: Option<&str>,
    ) -> Result<Admitted, AppError> {
        let core = self.gproxy().core().snapshot();
        let all_providers: BTreeSet<String> = core
            .providers
            .keys()
            .filter(|id| {
                request
                    .provider_id
                    .as_ref()
                    .is_none_or(|selected| selected == *id)
            })
            .cloned()
            .collect();
        let all_credentials: BTreeSet<String> = core.credentials.keys().cloned().collect();
        let admission = AdmissionRequest::new(
            caller,
            request.operation,
            &request.parts.headers,
            &request.request_id,
            &all_providers,
            &all_credentials,
        )
        .model(model)
        .body(json)
        .agent_session_id(request.agent_session_id.as_deref());
        self.admission(data).admit(admission).await
    }
}

/// The body as JSON, for the model name and the session ladder. A body that is
/// not JSON carries neither, which is not an error: a channel may still know
/// what to do with it.
fn json_body(body: &Bytes) -> Option<Value> {
    (!body.is_empty())
        .then(|| serde_json::from_slice(body).ok())
        .flatten()
}

/// The `model` field every JSON dialect this gateway speaks puts at the top
/// level. The handle parses the same field from the same bytes; its helper is
/// private, so this is the second copy rather than a call.
fn body_model(json: Option<&Value>) -> Option<&str> {
    json?.get("model")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_model_is_read_from_the_body_when_the_host_names_none() {
        let body = Bytes::from(serde_json::to_vec(&json!({"model": "test/m1"})).unwrap());
        let json = json_body(&body);
        assert_eq!(body_model(json.as_ref()), Some("test/m1"));
    }

    #[test]
    fn a_body_that_is_not_json_or_names_no_model_carries_neither() {
        assert_eq!(json_body(&Bytes::new()), None);
        assert_eq!(json_body(&Bytes::from_static(b"not json")), None);
        let body = Bytes::from(serde_json::to_vec(&json!({"input": "hi"})).unwrap());
        assert_eq!(body_model(json_body(&body).as_ref()), None);
        // A non-string model is not a model name.
        let body = Bytes::from(serde_json::to_vec(&json!({"model": 7})).unwrap());
        assert_eq!(body_model(json_body(&body).as_ref()), None);
    }
}
