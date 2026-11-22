//! Sending a request: as the client wrote it when the upstream speaks its
//! dialect, through one of protocol's conversion flows when it does not. The
//! choice is `gproxy_convert::default_route_for_model`, the full engine's.

use std::sync::{Arc, Mutex};

use gproxy_channel::ChannelBinding;
use gproxy_convert::{Call, ClientRequest, Converted, Route, prepare};
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse,
    adapt::generate::claude_compat,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, Upstream, UpstreamConnection,
    },
};
use gproxy_upstream::UpstreamError;
use http::StatusCode;

use crate::{
    credential::{Held, RotatingClient},
    engine::{Addressed, CODEC, Inner, Refusal},
    renew::Renew,
    resources::Resources,
    store::{LIMITS, ProtocolState},
};

/// Upstream calls for one request: its provider and the credential it read.
#[derive(Clone)]
pub(crate) struct Bound {
    pub inner: Arc<Inner>,
    pub provider: usize,
    /// Replaced when a `401` made the request renew it.
    held: Arc<Mutex<Held>>,
}

impl Bound {
    pub async fn new(inner: &Arc<Inner>, provider: usize) -> Result<Self, Refusal> {
        let held = inner.credential(provider).await?;
        Ok(Self {
            inner: inner.clone(),
            provider,
            held: Arc::new(Mutex::new(held)),
        })
    }

    fn held(&self) -> Held {
        self.held.lock().expect("credential lock").clone()
    }

    fn binding<'a>(&'a self, held: &'a Held) -> ChannelBinding<'a> {
        let provider = &self.inner.providers[self.provider];
        ChannelBinding::new(
            provider.channel.as_ref(),
            self.inner.provider_view(self.provider),
            held.record.view(&provider.name),
            Arc::new(RotatingClient {
                inner: self.inner.clone(),
                provider: self.provider,
                held: held.clone(),
            }),
        )
        .state(Arc::new(
            self.inner
                .store
                .scoped(format!("channel/{}/", provider.name)),
        ))
    }

    /// One native call. A `401` to a credential that can be renewed renews
    /// it and sends once more.
    pub async fn native(
        &self,
        operation: OperationKey,
        mut wire: WireRequest<HttpBody>,
    ) -> Result<WireResponse<HttpBody>, UpstreamError> {
        claude_compat::prepare(operation, &mut wire);
        let replay = match &wire.body {
            HttpBody::Bytes(bytes) => Some(WireRequest {
                method: wire.method.clone(),
                path: wire.path.clone(),
                query: wire.query.clone(),
                headers: wire.headers.clone(),
                body: HttpBody::Bytes(bytes.clone()),
            }),
            HttpBody::Stream(_) => None,
        };
        let held = self.held();
        let response = self.binding(&held).send(operation, wire).await?;
        let provider = &self.inner.providers[self.provider];
        let renewable = provider
            .channel
            .credential_refresh()
            .is_some_and(|refresher| refresher.supports(&held.record.view(&provider.name)));
        let (Some(replay), true) = (
            replay,
            response.status == StatusCode::UNAUTHORIZED && renewable,
        ) else {
            return Ok(response);
        };
        let Ok(renewed) = self
            .inner
            .renew(self.provider, &held, Renew::Unauthorized)
            .await
        else {
            return Ok(response);
        };
        *self.held.lock().expect("credential lock") = renewed.clone();
        self.binding(&renewed).send(operation, replay).await
    }

    pub async fn connect(
        &self,
        operation: OperationKey,
        request: WireRequest<()>,
    ) -> Result<UpstreamConnection, UpstreamError> {
        let held = self.held();
        self.binding(&held).connect(operation, request).await
    }
}

fn capability(error: UpstreamError) -> CapabilityError {
    match error {
        UpstreamError::Transport(error) => error,
        UpstreamError::UnsupportedOperation(_) | UpstreamError::WrongTransport(_) => {
            CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                error.to_string(),
            )
        }
        UpstreamError::InvalidConfig(_) | UpstreamError::InvalidCredential => CapabilityError::new(
            CapabilityErrorKind::Invalid,
            CapabilityErrorStage::Start,
            error.to_string(),
        ),
        other => CapabilityError::new(
            CapabilityErrorKind::Transport,
            CapabilityErrorStage::Start,
            other.to_string(),
        ),
    }
}

impl Upstream for Bound {
    type Target = OperationKey;

    fn send<'a>(
        &'a self,
        target: &'a OperationKey,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move { self.native(*target, request).await.map_err(capability) })
    }

    fn connect<'a>(
        &'a self,
        target: &'a OperationKey,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            Bound::connect(self, *target, request)
                .await
                .map_err(capability)
        })
    }

    fn limits(&self) -> CapabilityLimits {
        LIMITS
    }
}

/// The engine's capabilities as the conversion drivers take them.
pub(crate) struct LiteHost;

impl gproxy_convert::Host for LiteHost {
    type Upstream = Bound;
    type State = ProtocolState;
    type StateScope = String;
    type Resources = Resources;
    type ResourceScope = ();

    fn spawn(task: gproxy_convert::Task) {
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
    }
}

pub(crate) async fn run(
    inner: &Arc<Inner>,
    addressed: Addressed,
    operation: OperationKey,
    mut wire: WireRequest<HttpBody>,
) -> Result<WireResponse<HttpBody>, Refusal> {
    let provider = &inner.providers[addressed.provider];
    let model = addressed.model.as_deref();
    let unsupported = |why: &str| {
        Refusal::new(
            StatusCode::NOT_IMPLEMENTED,
            format!(
                "{:?} / {:?} on provider `{}`: {why}",
                operation.operation, operation.dialect, provider.name
            ),
        )
    };
    let view = inner.provider_view(addressed.provider);
    let route =
        gproxy_convert::default_route_for_model(provider.channel.as_ref(), view, operation, model);
    let target = match route {
        Route::Unsupported => return Err(unsupported("the channel does not serve it")),
        // The channel answers some operations itself. What the full gateway
        // answers from its own model catalogue and tokenizer has no
        // counterpart here.
        Route::Local
            if !provider
                .channel
                .local_operations()
                .contains(&operation.operation) =>
        {
            return Err(unsupported(
                "the upstream has no such operation, and the lite engine has no local model catalogue or tokenizer",
            ));
        }
        Route::Passthrough | Route::Local => None,
        Route::TransformTo { target } => Some(target),
    };
    let bound = Bound::new(inner, addressed.provider).await?;
    let Some(target) = target else {
        if let Some(model) = model {
            prepare::apply_model(&mut wire, operation.dialect, model);
            prepare::apply_form_model(&mut wire, model, CODEC)
                .await
                .map_err(|error| Refusal::bad_request(error.to_string()))?;
        }
        prepare::drop_thinking_handles(operation, &mut wire);
        return Ok(bound.native(operation, wire).await?);
    };
    let state = ProtocolState(inner.store.clone());
    let scope = format!("protocol/{}/", provider.name);
    // No sessions here: every request is its own conversation.
    let request_id = gproxy_convert::random_id();
    let resources = Resources {
        bound: bound.clone(),
    };
    let call = Call::<LiteHost> {
        upstream: &bound,
        client: operation,
        target: target.dialect,
        model,
        request: ClientRequest::new(&wire),
        limits: CODEC,
        state_store: &state,
        state_scope: &scope,
        job_scope: &scope,
        resources: &resources,
        resource_scope: &(),
        conversation_key: &request_id,
        provider_id: &provider.name,
        provider_name: &provider.name,
        base_url: provider.base_url.as_deref(),
        models: &[],
        operation_id: &request_id,
        publishes_urls: false,
        now_ms: gproxy_convert::now_ms(),
        collect: operation.operation == gproxy_protocol::Operation::GenerateContent
            && target.operation == gproxy_protocol::Operation::StreamGenerateContent,
        synthesize: operation.operation == gproxy_protocol::Operation::StreamGenerateContent
            && target.operation == gproxy_protocol::Operation::GenerateContent,
    };
    Ok(match gproxy_convert::dispatch(&call).await? {
        Converted::Success(response) | Converted::Rejected(response) => response,
        Converted::Stream(response) => WireResponse {
            status: response.status,
            headers: response.headers,
            body: HttpBody::Stream(response.body),
        },
    })
}
