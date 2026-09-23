//! Sending a request through the resolved plan.
//!
//! The builder collects what core needs for a `RequestContext` — scope,
//! attribution, budgets, session, deadline — resolves the model name, and then
//! walks the plan's targets. Core already fails over *within* one provider's
//! credential set; the loop here is the other axis, moving to the next
//! provider only for failures another provider could plausibly serve.
//!
//! # What stops the loop
//!
//! | Outcome | Next provider | Why |
//! |---|---|---|
//! | `NoUsableCredential`, `CredentialDead`, `RefreshContended` | yes | This provider has nothing to spend; another may |
//! | `ContinuationElsewhere` | yes | The pinned instance holds it; nothing here is wrong |
//! | any `Channel(..)` error | yes | A channel error is scoped to the provider that raised it |
//! | a 401, 403, 429 or 5xx answer | yes | Core already exhausted this provider's credentials |
//! | `BudgetExhausted`, `Cancelled`, `DeadlineExceeded`, `Forbidden` | no | The caller's own limit; retrying spends more of it |
//! | `Transform`, `Route`, `Rewrite`, `OperationMismatch`, `InvalidTarget`, `NotImplemented` | no | The request or the configuration is wrong everywhere |
//! | `Store`, `Cache`, `Secret`, `Limits`, `Assembly`, `File` | no | This instance is broken, not the provider |
//!
//! The attempt budget is shared across targets: each target is granted at most
//! as many attempts as it has credentials, never more than what is left, and
//! what it was granted is subtracted whether or not it used all of it. A plan
//! therefore cannot cost more upstream calls than its `max_attempts`.

use std::{collections::BTreeSet, num::NonZeroU32, sync::Arc, time::Duration};

use futures_util::StreamExt;
use gproxy_core::{
    BudgetOwner, CoreData, CoreError, ExecutionTarget, HttpExecution, RequestContext,
    SessionIdentity, SessionSource, UsageAttribution, WebSocketExecution,
};
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest,
    capability::UpstreamConnection,
    connection::{Bytes, HeaderMap, StatusCode},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use web_time::Instant;

use crate::{
    SdkError, SdkResult,
    handle::Gproxy,
    ids::random_id,
    resolve::{Plan, ResolveRequest, Target},
    rt::now_ms,
    session,
};

/// Everything both builders collect. Split out so the setters, the
/// preparation and the failover loop exist once.
#[derive(Default)]
struct Options {
    scope: Option<String>,
    attribution: UsageAttribution,
    budgets: Vec<BudgetOwner>,
    session: Option<SessionIdentity>,
    channel: Option<String>,
    providers: Option<BTreeSet<String>>,
    credentials: Option<BTreeSet<String>>,
    max_attempts: Option<NonZeroU32>,
    deadline: Option<Duration>,
    cancellation: Option<CancellationToken>,
    model: Option<String>,
    request_id: Option<String>,
    agent_session_id: Option<String>,
}

/// The setters, generated for both builders from one list so the two entry
/// points cannot drift apart.
macro_rules! setters {
    ($builder:ident) => {
        impl<C> $builder<'_, C> {
            /// The caller's isolation scope, as the application layer decided
            /// it. **Required**: core uses it to keep one caller's credential
            /// affinity out of another's, and there is no safe default.
            pub fn scope(mut self, scope: impl Into<String>) -> Self {
                self.options.scope = Some(scope.into());
                self
            }
            /// Historical caller facts recorded with the usage row. Never
            /// inferred from the scope.
            pub fn attribution(mut self, attribution: UsageAttribution) -> Self {
                self.options.attribution = attribution;
                self
            }
            /// The budget owners this request spends for, innermost first.
            pub fn budgets(mut self, budgets: Vec<BudgetOwner>) -> Self {
                self.options.budgets = budgets;
                self
            }
            /// An identity the caller already extracted. Wins over anything in
            /// the request.
            pub fn session(mut self, session: SessionIdentity) -> Self {
                self.options.session = Some(session);
                self
            }
            /// An explicit gateway session id, equivalent to the
            /// `x-gproxy-session-id` header.
            pub fn session_id(mut self, id: impl Into<String>) -> Self {
                self.options.session = Some(SessionIdentity {
                    id: id.into(),
                    source: SessionSource::Gateway,
                    field: None,
                    agent_session_id: None,
                });
                self
            }
            /// The `agent_sessions` row this request runs under, when the host
            /// manages the session's credential binding as an assignment.
            pub fn agent_session_id(mut self, id: impl Into<String>) -> Self {
                self.options.agent_session_id = Some(id.into());
                self
            }
            /// Restrict resolution to one channel id, as a channel-specific
            /// ingress mount does.
            pub fn channel(mut self, channel: impl Into<String>) -> Self {
                self.options.channel = Some(channel.into());
                self
            }
            /// Intersect the plan with the provider ids this caller may reach.
            pub fn providers(mut self, providers: BTreeSet<String>) -> Self {
                self.options.providers = Some(providers);
                self
            }
            /// Intersect the plan with the credential ids this caller may
            /// spend. This is how an application layer keeps one organization's
            /// credentials out of another's requests.
            pub fn credentials(mut self, credentials: BTreeSet<String>) -> Self {
                self.options.credentials = Some(credentials);
                self
            }
            /// Lower the attempt budget the plan came with. It is never
            /// raised: a route's budget is a ceiling.
            pub fn max_attempts(mut self, attempts: NonZeroU32) -> Self {
                self.options.max_attempts = Some(attempts);
                self
            }
            /// Wall clock for the whole call, across every target.
            pub fn deadline(mut self, within: Duration) -> Self {
                self.options.deadline = Some(within);
                self
            }
            pub fn cancellation(mut self, token: CancellationToken) -> Self {
                self.options.cancellation = Some(token);
                self
            }
            /// The model name to resolve. Without one the body's `model` field
            /// is used, when it has a JSON body with one.
            pub fn model(mut self, model: impl Into<String>) -> Self {
                self.options.model = Some(model.into());
                self
            }
            /// The id this request is observed and logged under. Random when
            /// not supplied.
            pub fn request_id(mut self, id: impl Into<String>) -> Self {
                self.options.request_id = Some(id.into());
                self
            }
        }
    };
}

/// An HTTP call, waiting for its scope.
pub struct CallBuilder<'a, C> {
    gproxy: &'a Gproxy<C>,
    operation: OperationKey,
    request: WireRequest<HttpBody>,
    options: Options,
}

/// A websocket handshake, waiting for its scope.
pub struct ConnectBuilder<'a, C> {
    gproxy: &'a Gproxy<C>,
    operation: OperationKey,
    request: WireRequest<()>,
    options: Options,
}

setters!(CallBuilder);
setters!(ConnectBuilder);

impl<C> Gproxy<C> {
    /// Start an HTTP call. Nothing happens until `send`.
    pub fn call(&self, key: OperationKey, request: WireRequest<HttpBody>) -> CallBuilder<'_, C> {
        CallBuilder {
            gproxy: self,
            operation: key,
            request,
            options: Options::default(),
        }
    }

    /// Start a websocket handshake. Nothing happens until `send`.
    pub fn connect(&self, key: OperationKey, request: WireRequest<()>) -> ConnectBuilder<'_, C> {
        ConnectBuilder {
            gproxy: self,
            operation: key,
            request,
            options: Options::default(),
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> CallBuilder<'_, C> {
    /// Resolve, then walk the plan.
    ///
    /// The body is buffered once so it can be replayed against each target. A
    /// streaming body larger than `max_request_body_bytes` cannot be replayed;
    /// rather than failing the request, the plan is cut down to its first
    /// target and the body is forwarded as a stream.
    pub async fn send(self) -> SdkResult<HttpExecution> {
        let Self {
            gproxy,
            operation,
            request,
            options,
        } = self;
        let WireRequest {
            method,
            path,
            query,
            mut headers,
            body,
        } = request;
        let snapshot = gproxy.core().snapshot();
        let mut payload = Payload::buffer(body, snapshot.limits.max_request_body_bytes).await;
        let json = payload.json();
        let mut prepared = options
            .prepare(gproxy, snapshot, operation, &headers, json.as_ref())
            .await?;
        if !payload.replayable() && prepared.plan.targets.len() > 1 {
            prepared.plan.targets.truncate(1);
        }
        session::strip_gateway_header(&mut headers);

        let mut walk = Walk::new(prepared);
        while let Some(step) = walk.next() {
            let request = WireRequest {
                method: method.clone(),
                path: path.clone(),
                query: query.clone(),
                headers: headers.clone(),
                body: payload.take(),
            };
            match gproxy.core().send(step.context, request).await {
                Ok(execution) => {
                    let status = execution.response().status;
                    if step.more && failover_status(status) {
                        // Dropping the execution settles its funnel as failed,
                        // which is what the next provider's attempt should see.
                        drop(execution);
                        walk.remember(upstream_error(&step.provider, status));
                        continue;
                    }
                    return Ok(execution);
                }
                Err(error) => walk.failed(error)?,
            }
        }
        Err(walk.exhausted())
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ConnectBuilder<'_, C> {
    /// Resolve, then walk the plan. A rejected handshake counts exactly as a
    /// rejected HTTP answer: its status decides whether another provider is
    /// worth trying.
    pub async fn send(self) -> SdkResult<WebSocketExecution> {
        let Self {
            gproxy,
            operation,
            request,
            options,
        } = self;
        let WireRequest {
            method,
            path,
            query,
            mut headers,
            body: (),
        } = request;
        let snapshot = gproxy.core().snapshot();
        let prepared = options
            .prepare(gproxy, snapshot, operation, &headers, None)
            .await?;
        session::strip_gateway_header(&mut headers);

        let mut walk = Walk::new(prepared);
        while let Some(step) = walk.next() {
            let request = WireRequest {
                method: method.clone(),
                path: path.clone(),
                query: query.clone(),
                headers: headers.clone(),
                body: (),
            };
            match gproxy.core().connect(step.context, request).await {
                Ok(execution) => {
                    let rejected = match execution.response() {
                        UpstreamConnection::Rejected(response) => Some(response.status),
                        UpstreamConnection::Connected { .. } => None,
                    };
                    if let Some(status) = rejected
                        && step.more
                        && failover_status(status)
                    {
                        drop(execution);
                        walk.remember(upstream_error(&step.provider, status));
                        continue;
                    }
                    return Ok(execution);
                }
                Err(error) => walk.failed(error)?,
            }
        }
        Err(walk.exhausted())
    }
}

/// Everything decided before the first target is tried, pinned for the call.
struct Prepared {
    snapshot: Arc<CoreData>,
    plan: Plan,
    operation: OperationKey,
    request_id: String,
    scope: String,
    attribution: UsageAttribution,
    budgets: Vec<BudgetOwner>,
    session: Option<SessionIdentity>,
    deadline: Option<Instant>,
    cancellation: CancellationToken,
    started_at_ms: i64,
    attempts: NonZeroU32,
}

impl Options {
    async fn prepare<C>(
        self,
        gproxy: &Gproxy<C>,
        snapshot: Arc<CoreData>,
        operation: OperationKey,
        headers: &HeaderMap,
        body: Option<&Value>,
    ) -> SdkResult<Prepared> {
        let scope = self.scope.ok_or_else(|| {
            SdkError::invalid("scope required: call scope() before send(), after admission")
        })?;
        let request_id = self.request_id.unwrap_or_else(random_id);
        let model = self
            .model
            .or_else(|| body?.get("model")?.as_str().map(str::to_owned));

        // The explicit identity first, then the request's own, then the
        // request id — which is honestly labelled as not being a session.
        let mut session = self
            .session
            .or_else(|| session::extract(headers, body, operation))
            .unwrap_or_else(|| SessionIdentity {
                id: request_id.clone(),
                source: SessionSource::RequestFallback,
                field: None,
                agent_session_id: None,
            });
        session.agent_session_id = self.agent_session_id.or(session.agent_session_id);

        let routing = gproxy.routing();
        let plan = gproxy
            .resolve_with(
                &snapshot,
                &routing,
                ResolveRequest {
                    model: model.as_deref(),
                    operation,
                    allowed_providers: self.providers.as_ref(),
                    allowed_credentials: self.credentials.as_ref(),
                    channel: self.channel.as_deref(),
                    // A request-level fallback is not a session and must not
                    // pin a credential to it.
                    affinity_key: session.is_stable().then_some(session.id.as_str()),
                },
            )
            .await?;

        let mut attribution = self.attribution;
        attribution.model = attribution.model.or(model);
        // A caller ceiling only lowers the plan's budget.
        let attempts = match self.max_attempts {
            Some(limit) => limit.min(plan.max_attempts),
            None => plan.max_attempts,
        };
        Ok(Prepared {
            snapshot,
            plan,
            operation,
            request_id,
            scope,
            attribution,
            budgets: self.budgets,
            session: Some(session),
            deadline: self.deadline.map(|within| Instant::now() + within),
            cancellation: self.cancellation.unwrap_or_default(),
            started_at_ms: now_ms(),
            attempts,
        })
    }
}

/// One target of the plan, with the context core will execute it under.
struct Step {
    context: Arc<RequestContext>,
    provider: String,
    /// Whether a further target is reachable with the budget that is left.
    more: bool,
}

/// The cross-provider walk: the shared attempt budget, the remembered failure
/// and the target cursor.
struct Walk {
    prepared: Prepared,
    targets: std::vec::IntoIter<Target>,
    remaining_targets: usize,
    attempts_left: u32,
    remembered: Option<SdkError>,
}

impl Walk {
    fn new(mut prepared: Prepared) -> Self {
        let targets = std::mem::take(&mut prepared.plan.targets);
        Self {
            remaining_targets: targets.len(),
            attempts_left: prepared.attempts.get(),
            targets: targets.into_iter(),
            prepared,
            remembered: None,
        }
    }

    /// The next target, or `None` when the plan or the budget is spent. The
    /// grant is subtracted up front, so a target that used fewer attempts than
    /// it was allowed still cannot make the plan exceed its ceiling.
    fn next(&mut self) -> Option<Step> {
        if self.attempts_left == 0 {
            return None;
        }
        let target = self.targets.next()?;
        self.remaining_targets = self.remaining_targets.saturating_sub(1);
        let credentials = u32::try_from(target.credentials.len()).unwrap_or(u32::MAX);
        let granted = self.attempts_left.min(credentials).max(1);
        self.attempts_left = self.attempts_left.saturating_sub(granted);
        let provider = target.provider.entity.id.clone();
        let more = self.remaining_targets > 0 && self.attempts_left > 0;
        Some(Step {
            context: self.prepared.context(target, granted),
            provider,
            more,
        })
    }

    fn remember(&mut self, error: SdkError) {
        self.remembered = Some(error);
    }

    /// Classify one core failure: either it ends the call, or it is remembered
    /// and the walk continues.
    fn failed(&mut self, error: CoreError) -> SdkResult<()> {
        if stops_the_call(&error) {
            return Err(error.into());
        }
        self.remembered = Some(error.into());
        Ok(())
    }

    /// What the caller sees when no target answered.
    fn exhausted(self) -> SdkError {
        self.remembered
            .unwrap_or(SdkError::Core(CoreError::NoUsableCredential))
    }
}

impl Prepared {
    fn context(&self, target: Target, attempts: u32) -> Arc<RequestContext> {
        Arc::new(RequestContext {
            request_id: self.request_id.clone(),
            attribution: self.attribution.clone(),
            snapshot: self.snapshot.clone(),
            scope: self.scope.clone(),
            session: self.session.clone(),
            operation: self.operation,
            target: ExecutionTarget {
                requested_model: target.requested_model,
                provider: target.provider,
                upstream_model: target.upstream_model,
                credentials: target.credentials,
            },
            budgets: self.budgets.clone(),
            max_attempts: NonZeroU32::new(attempts).unwrap_or(NonZeroU32::MIN),
            started_at_ms: self.started_at_ms,
            deadline: self.deadline,
            cancellation: self.cancellation.clone(),
        })
    }
}

/// Answers that another provider could plausibly do better on: an exhausted
/// or rejected credential, and anything the upstream itself failed at. Core
/// has already tried this provider's other credentials by the time one of
/// these comes back.
fn failover_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
}

fn upstream_error(provider: &str, status: StatusCode) -> SdkError {
    SdkError::Upstream {
        status: status.as_u16(),
        body: format!("provider `{provider}` answered {status}"),
    }
}

/// Whether this failure is about the caller or this instance rather than the
/// provider. Exhaustive on purpose: a new `CoreError` variant has to be
/// classified here rather than silently falling into a retry.
fn stops_the_call(error: &CoreError) -> bool {
    use CoreError as E;
    match error {
        // The provider had nothing to spend, or the material it had is gone.
        E::NoUsableCredential | E::CredentialDead { .. } | E::RefreshContended { .. } => false,
        // The continuation lives elsewhere; this provider is simply the wrong
        // address for it.
        E::ContinuationElsewhere { .. } => false,
        // Every channel error is raised while talking to one provider, so it
        // says nothing about the next one.
        E::Channel(_) => false,

        // The caller's own limits. Trying another provider spends more of
        // exactly what has run out.
        E::BudgetExhausted { .. } | E::Forbidden(_) | E::Cancelled | E::DeadlineExceeded => true,
        // The request or the configuration is wrong wherever it is sent.
        E::Transform(_)
        | E::Route(_)
        | E::Rewrite(_)
        | E::OperationMismatch { .. }
        | E::InvalidTarget(_)
        | E::NotImplemented(_) => true,
        // This instance is broken; the providers are not.
        E::Store(_) | E::Cache(_) | E::Secret(_) | E::Limits(_) | E::Assembly(_) | E::File(_) => {
            true
        }
    }
}

/// The request body, buffered once so every target can be sent the same bytes.
enum Payload {
    Bytes(Bytes),
    /// Too large to buffer, or already failing: forwarded once, to one target.
    Stream(Option<HttpBody>),
}

impl Payload {
    /// Read a streaming body into memory, up to the snapshot's request cap.
    /// Past the cap the prefix is put back in front of the rest and the body
    /// stays a stream: a 40 MiB upload is not worth failing over.
    async fn buffer(body: HttpBody, max_bytes: u64) -> Self {
        match body {
            HttpBody::Bytes(bytes) => Self::Bytes(bytes),
            HttpBody::Stream(mut stream) => {
                let mut collected: Vec<Bytes> = Vec::new();
                let mut total = 0u64;
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(chunk) => {
                            total += chunk.len() as u64;
                            collected.push(chunk);
                            if total > max_bytes {
                                let prefix =
                                    futures_util::stream::iter(collected.into_iter().map(Ok));
                                return Self::Stream(Some(HttpBody::Stream(Box::pin(
                                    prefix.chain(stream),
                                ))));
                            }
                        }
                        Err(error) => {
                            // Replay what arrived and let the error surface
                            // where the upstream would have seen it.
                            let prefix = futures_util::stream::iter(collected.into_iter().map(Ok));
                            let failed = futures_util::stream::iter([Err(error)]);
                            return Self::Stream(Some(HttpBody::Stream(Box::pin(
                                prefix.chain(failed),
                            ))));
                        }
                    }
                }
                let mut joined = Vec::with_capacity(total as usize);
                for chunk in collected {
                    joined.extend_from_slice(&chunk);
                }
                Self::Bytes(Bytes::from(joined))
            }
        }
    }

    fn replayable(&self) -> bool {
        !matches!(self, Self::Stream(_))
    }

    /// The buffered body as JSON, for the model name and the session fields.
    /// A body that is not JSON is not an error here; it simply carries neither.
    fn json(&self) -> Option<Value> {
        match self {
            Self::Bytes(bytes) if !bytes.is_empty() => serde_json::from_slice(bytes).ok(),
            _ => None,
        }
    }

    fn take(&mut self) -> HttpBody {
        match self {
            Self::Bytes(bytes) => HttpBody::Bytes(bytes.clone()),
            Self::Stream(stream) => stream
                .take()
                .unwrap_or_else(|| HttpBody::Bytes(Bytes::new())),
        }
    }
}
