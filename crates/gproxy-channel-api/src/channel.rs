//! The channel trait and its request/response views.

use bytes::Bytes;
use gproxy_protocol::{OperationKey, StreamFraming};
use http::{Request, StatusCode};
use serde_json::Value;

use crate::BoxFuture;
use crate::disposition::Disposition;
use crate::login::ChannelLoginRef;
use crate::operation::OperationDriver;
use crate::resource::{ResourceCtx, ResourceMutation};
use crate::session::SessionPreparer;
use crate::surface::{SurfaceRequest, SurfaceTable};
use crate::usage::NormalizedUsage;
use crate::wire::MaybeSync;

pub use crate::prepare::{PrepareCtx, PreparedRequest};

pub use crate::channel_error::ChannelError;

pub use crate::channel_descriptor::{
    ChannelDescriptor, ChannelField, ChannelFieldControl, ChannelRouteAction, ChannelSupport,
    ChannelTrafficPolicy,
};

/// What classification may read. For streaming responses the body is
/// whatever error page arrived before streaming began, or empty.
pub struct ResponseView<'a> {
    pub status: StatusCode,
    pub headers: &'a http::HeaderMap,
    pub body: &'a [u8],
}

/// Context for constructing a per-response stream decoder. Usage observers
/// may need request parameters (audio format) and response metadata while
/// still returning an owned state machine.
pub struct StreamCtx<'a> {
    pub key: OperationKey,
    pub framing: StreamFraming,
    pub request_body: &'a Bytes,
    pub response_headers: &'a http::HeaderMap,
}

/// The complete buffered exchange visible to usage extraction.
pub struct UsageCtx<'a> {
    pub key: OperationKey,
    pub request_body: &'a Bytes,
    pub response_headers: &'a http::HeaderMap,
    pub response_body: &'a [u8],
}

/// Raw buffered upstream response visible to channel-private normalization
/// before any protocol-pair conversion. Capture and usage still consume the
/// unshaped bytes.
pub struct ResponseShapeCtx<'a> {
    pub key: OperationKey,
    pub status: StatusCode,
    pub headers: &'a http::HeaderMap,
    pub body: &'a Bytes,
}

/// One decoded stream frame, zero-copy where the wire allows.
#[derive(Debug)]
pub struct Frame(pub Bytes);

/// What a finished stream reports.
#[derive(Debug, Default)]
pub struct StreamTail {
    /// Frames completed only when the decoder observed EOF, such as an SSE
    /// event whose final blank-line delimiter was omitted.
    pub frames: Vec<Frame>,
    pub usage: Option<NormalizedUsage>,
    /// Provider-reported serving tier, independent of whether the event also
    /// carried usage.
    pub actual_service_tier: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEnd {
    Complete,
    Interrupted,
}

/// Stateful per-response stream decoder (SSE, AWS event-stream, ...).
/// A pure state machine: owned chunks in, frames out, tail at the end. Owning
/// the chunk lets an observe-only decoder relay it as a [`Frame`] without a
/// copy while still collecting usage state.
pub trait StreamDecoder: Send {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError>;
    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError>;
}

/// Minimal buffered HTTP the engine lends to `refresh` — refresh calls are
/// small JSON exchanges; no streaming, no zero-copy concern.
pub trait SimpleHttp: MaybeSync {
    fn send<'a>(
        &'a self,
        request: Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>>;

    fn wait<'a>(&'a self, _duration: std::time::Duration) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

/// The contract. Synchronous and object-safe on purpose: adapters are pure
/// logic; I/O and state live in the engine and the host.
pub trait Channel: Send + Sync {
    fn descriptor(&self) -> &'static ChannelDescriptor;

    /// Every route this channel declares, in priority order. The first row
    /// for a source is the provider default; further rows for the same source
    /// are additional executable paths a routing override or a
    /// [`Channel::select_support`] override may reach. Provider defaults are
    /// policy, not a capability inference. Hosts materialize this table
    /// without changing operator-owned cells.
    fn routing_table(&self) -> &'static [ChannelSupport];

    /// Models supplied by a local ListModels implementation for one credential.
    fn local_models(&self, _secret: &Value) -> Option<Vec<crate::model::ModelInfo>> {
        None
    }

    /// First-time credential acquisition when this channel supports it.
    fn login(&self) -> Option<ChannelLoginRef<'_>> {
        None
    }

    /// Select one declared route after the credential secret is available.
    /// Most channels have one target per source; merged credential families
    /// may choose among duplicate source rows by secret shape.
    fn select_support(&self, source: OperationKey, secret: &Value) -> Option<ChannelSupport> {
        let _ = secret;
        default_route(self, source)
    }

    /// Build the upstream request: URL, auth injection, header allow-list,
    /// body shaping. Must not perform I/O.
    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError>;

    fn claude_fallback(&self) -> Option<crate::ClaudeFallbackCapabilities> {
        None
    }

    fn fallback_model(&self, _primary: &str, model: &str) -> String {
        model.to_owned()
    }

    /// A transform-after driver for a multi-call operation. The driver is a
    /// pure state machine; the core performs and funnels every emitted call.
    fn operation_driver(
        &self,
        ctx: PrepareCtx<'_>,
    ) -> Result<Option<Box<dyn OperationDriver>>, ChannelError> {
        let _ = ctx;
        Ok(None)
    }

    fn classify(&self, response: ResponseView<'_>) -> Disposition;

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        let _ = ctx;
        None
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage>;

    fn observe_quota(&self, headers: &http::HeaderMap) -> Vec<crate::usage::QuotaObservation> {
        let _ = headers;
        Vec::new()
    }

    fn quota_capabilities(&self, secret: &Value) -> Option<crate::QuotaCapabilities> {
        let _ = secret;
        None
    }

    fn quota_fields(&self) -> &'static [ChannelField] {
        &[]
    }

    fn quota_sources(&self, secret: &Value, _settings: &Value) -> Vec<crate::QuotaSource> {
        let probe = self
            .quota_capabilities(secret)
            .is_some_and(|value| value.probe);
        vec![crate::QuotaSource {
            id: if probe { "subscription" } else { "quota" }.into(),
            label: if probe {
                "Subscription quota"
            } else {
                "Upstream quota"
            }
            .into(),
            kinds: if probe {
                vec![crate::QuotaKind::Window]
            } else {
                Vec::new()
            },
            mode: if probe {
                crate::QuotaQueryMode::Probe
            } else {
                crate::QuotaQueryMode::Unavailable
            },
            support: if probe {
                crate::QuotaSupport::Ready
            } else {
                crate::QuotaSupport::Unsupported
            },
            reason: (!probe)
                .then(|| "No supported upstream quota query is available for this channel.".into()),
            automatic: probe,
        }]
    }

    fn prepare_quota_source(
        &self,
        source_id: &str,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<Request<Bytes>>, ChannelError> {
        if source_id == "subscription" {
            self.prepare_quota_probe(secret, settings)
        } else {
            Ok(None)
        }
    }

    fn parse_quota_source(
        &self,
        source_id: &str,
        status: StatusCode,
        _headers: &http::HeaderMap,
        body: &[u8],
    ) -> Result<Vec<crate::QuotaEntry>, ChannelError> {
        if source_id != "subscription" {
            return Err(ChannelError::Prepare("unknown quota source".into()));
        }
        let windows = self.parse_quota_probe(status, body);
        if windows.is_empty() {
            return Err(ChannelError::Prepare("invalid quota response".into()));
        }
        Ok(windows
            .iter()
            .map(|window| crate::QuotaEntry::from_window(window, 0))
            .collect())
    }

    fn prepare_quota_source_page(
        &self,
        source_id: &str,
        secret: &Value,
        settings: &Value,
        cursor: Option<&str>,
    ) -> Result<Option<Request<Bytes>>, ChannelError> {
        if cursor.is_some() {
            return Err(ChannelError::Prepare(
                "quota pagination is unsupported".into(),
            ));
        }
        self.prepare_quota_source(source_id, secret, settings)
    }

    fn parse_quota_source_page(
        &self,
        source_id: &str,
        status: StatusCode,
        headers: &http::HeaderMap,
        body: &[u8],
    ) -> Result<crate::QuotaSourcePage, ChannelError> {
        Ok(crate::QuotaSourcePage {
            entries: self.parse_quota_source(source_id, status, headers, body)?,
            next_cursor: None,
        })
    }

    fn quota_response_entries(
        &self,
        _headers: &http::HeaderMap,
        _upstream_model: &str,
    ) -> Vec<crate::QuotaEntry> {
        Vec::new()
    }

    fn prepare_quota_probe(
        &self,
        secret: &Value,
        provider_settings: &Value,
    ) -> Result<Option<Request<Bytes>>, ChannelError> {
        let _ = (secret, provider_settings);
        Ok(None)
    }

    fn parse_quota_probe(
        &self,
        status: StatusCode,
        body: &[u8],
    ) -> Vec<crate::usage::QuotaObservation> {
        let _ = (status, body);
        Vec::new()
    }

    /// Richer reset-credit details when the channel has a dedicated credits
    /// endpoint (per-credit expiry). Fired after the usage probe; its
    /// response also goes through [`Channel::parse_quota_probe_credits`].
    fn prepare_quota_credits_probe(
        &self,
        _secret: &Value,
        _provider_settings: &Value,
    ) -> Result<Option<Request<Bytes>>, ChannelError> {
        Ok(None)
    }

    /// Parses either the usage-probe body or the credits-probe body,
    /// whichever the channel recognizes.
    fn parse_quota_probe_credits(
        &self,
        _status: StatusCode,
        _body: &[u8],
    ) -> Option<crate::usage::QuotaResetCredits> {
        None
    }

    fn prepare_quota_reset(
        &self,
        _secret: &Value,
        _provider_settings: &Value,
        _redeem_request_id: &str,
    ) -> Result<Option<Request<Bytes>>, ChannelError> {
        Ok(None)
    }

    fn parse_quota_reset(
        &self,
        _status: StatusCode,
        _body: &[u8],
    ) -> Option<crate::usage::QuotaResetResult> {
        None
    }

    /// Prepare the trusted observer for a successful long-lived session.
    /// The channel owns Location parsing, authentication, and event-meter
    /// construction; core owns the socket and final settlement.
    fn session_preparer(&self) -> Option<SessionPreparer> {
        None
    }

    /// Whether an asynchronous operation poll is a successful billable
    /// terminal response. The operation spec decides when this hook applies.
    fn settlement_ready(&self, ctx: UsageCtx<'_>) -> Result<bool, ChannelError> {
        let _ = ctx;
        Ok(false)
    }

    /// Extract durable resource binding changes from a successful native
    /// response. Persistence and owner/provider scoping remain in the core.
    fn resource_mutations(
        &self,
        ctx: ResourceCtx<'_>,
    ) -> Result<Vec<ResourceMutation>, ChannelError> {
        let _ = ctx;
        Ok(Vec::new())
    }

    /// Normalize a channel-private buffered envelope into its declared native
    /// target wire before the pairwise outward transform.
    fn shape_response(&self, ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
        Ok(ctx.body.clone())
    }

    /// Unix time after which the secret should be refreshed proactively;
    /// `None` = this channel's credentials never refresh.
    fn refresh_due(&self, secret: &Value) -> Option<i64> {
        let _ = secret;
        None
    }

    /// Refresh the secret. Returns the full replacement secret; the engine
    /// persists it through the host's version-guarded `CredentialStore`.
    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        provider_settings: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, ChannelError>>> {
        let _ = (secret, provider_settings, http);
        None
    }

    /// Prepare a provider control-plane request declared by a surface entry.
    /// These paths have no [`OperationKey`], so they cannot use
    /// [`Channel::prepare`].
    fn prepare_surface(
        &self,
        request: &SurfaceRequest,
        websocket: bool,
        provider_settings: &Value,
        secret: &Value,
    ) -> Result<PreparedRequest, ChannelError> {
        let _ = (request, websocket, provider_settings, secret);
        Err(ChannelError::Prepare(
            "channel does not prepare surface requests".into(),
        ))
    }

    /// The service-surface table this channel brings (emulated vendor
    /// control-plane endpoints). Upstream path knowledge stays here — v2
    /// kept the `/wham/...` map in the HTTP layer and paid for it twice.
    fn surfaces(&self) -> SurfaceTable {
        SurfaceTable(&[])
    }

    fn requires_continuations(&self) -> bool {
        false
    }
}

/// The capability view of [`Channel::routing_table`]: every route the channel
/// can actually run. `Local` and `Unsupported` rows are operator-facing
/// policy, so they are never executable paths.
pub fn executable_routes(
    channel: &(impl Channel + ?Sized),
) -> impl Iterator<Item = ChannelSupport> {
    channel.routing_table().iter().copied().filter(|support| {
        matches!(
            support.action,
            ChannelRouteAction::Passthrough | ChannelRouteAction::TransformTo
        )
    })
}

/// The provider default for one source: the first declared row, honoured only
/// when it is executable. Answers what the channel does before a credential is
/// in hand; [`Channel::select_support`] is the secret-aware form.
pub fn default_route(
    channel: &(impl Channel + ?Sized),
    source: OperationKey,
) -> Option<ChannelSupport> {
    let route = channel
        .routing_table()
        .iter()
        .find(|support| support.source == source)?;
    matches!(
        route.action,
        ChannelRouteAction::Passthrough | ChannelRouteAction::TransformTo
    )
    .then_some(*route)
}
