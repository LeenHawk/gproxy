---
title: "Embedding the Core"
description: "Crate graph, execution tiers, host traits and their guarantees, boundary types, wasm notes, gproxy-app as reference embedder, and the standalone libraries"
---

`gproxy-core` is a library. The `gproxy` binary is one host of it, the
edge bundle is another, and your application can be a third. Nothing in
the workspace is published to crates.io; embedding means a git or path
dependency on this repository:

```toml
[dependencies]
gproxy-core = { git = "https://github.com/LeenHawk/gproxy", branch = "3.0" }
gproxy-channels = { git = "https://github.com/LeenHawk/gproxy", branch = "3.0" }
```

The workspace is Rust edition 2024 at version `3.0.0`. The public
surface is not semver-stable yet.

## Crate Graph

| Crate | One line |
| --- | --- |
| `gproxy-protocol` | Operation taxonomy, the `OperationSpec` registry, and the OpenAI, Claude, Gemini and AWS wire models. `serde` and `http` only; builds for wasm. |
| `gproxy-transform` | Pure pairwise request, response and stream transforms between the wire families. |
| `gproxy-channel-api` | The channel contract: `Channel`, service surfaces, `NormalizedUsage`, `BindingStore`, and the wire primitives `ByteStream`, `WsDuplex`, `BoxFuture`. |
| `gproxy-channels` | The built-in provider adapters (28 channels; `claudeweb` is native-only). |
| `gproxy-core` | The engine: `Core`, host traits, boundary types, the control-plane read model, the settlement funnel, rules. The one crate an embedder must depend on. |
| `gproxy-upstream` | The canonical `UpstreamTransport`: `WreqTransport` on native (TLS profiles, proxies, WebSocket), `FetchTransport` on wasm. |
| `gproxy-tokenize` | Offline token counting: tiktoken, Hugging Face vocabularies, character estimate. |
| `gproxy-store` | Schema catalog, migrations, the four SQL backends and the cache backends. |
| `gproxy-admin` | Framework-free admin and portal dispatch over `http` types; DTOs exported to TypeScript with `ts-rs`. |
| `gproxy-app` | The reference embedder: config, bootstrap, the ArcSwap snapshot control plane, host services, v2 migration. |
| `gproxy-host-axum` | The native listener and the `gproxy` binary: axum, static assets, autostart, self-update, announcements. |
| `gproxy-host-edge` | The fetch-based wasm host for Cloudflare, Deno and Netlify. |

Dependencies point one way. Hosts depend on `gproxy-app`; `gproxy-app`
depends on `gproxy-core`, `gproxy-store` and `gproxy-admin`; `gproxy-core`
depends on `gproxy-channel-api`, `gproxy-channels`, `gproxy-transform`
and `gproxy-protocol`. The core never depends on a server framework, a
database, or a UI.

## The Two Tiers

From `crates/gproxy-core/src/api.rs`:

```rust
pub struct Core<H: Host> { /* private */ }

impl<H: Host> Core<H> {
    pub fn new(host: H, channels: ChannelRegistry) -> Result<Self, InitError>

    pub async fn invoke(
        &self,
        control: &dyn ControlPlane,
        target: &Target,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError>

    pub async fn execute(
        &self,
        control: &dyn ControlPlane,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError>

    pub async fn execute_planned(
        &self,
        control: &dyn ControlPlane,
        ctx: RequestCtx,
        plan: Plan,
    ) -> Result<ExecOutcome, CoreError>

    pub fn matches_ingress(&self, method: &http::Method, path: &str, upgrade: bool) -> bool
}
```

- **Tier 1, `invoke`.** One request, in a wire shape the target's channel
  speaks natively, on one chosen credential. No routing, no transform, no
  failover; token refresh on expiry and the full settlement funnel still
  apply. Service-surface forwards use exactly this.
- **Tier 2, `execute`.** The full engine: classify, authenticate, alias
  and variant preprocessing, resolve a `Plan` from the `ControlPlane`,
  admit, transform, channel, transport, failover, funnel.
- **Tier 2 with your plan, `execute_planned`.** Skip resolution and hand
  the engine the ordered targets and budget yourself. Classification,
  transforms, failover inside the budget and settlement still run.

`Core::new` refuses at startup, not mid-traffic, when a registered channel
needs a host capability the host does not provide:

```rust
pub enum InitError {
    SurfacesWithoutBindings { channel: &'static str },
    ResourceAffinityWithoutBindings { channel: &'static str },
    ContinuationsUnavailable { channel: &'static str },
    ContinuationSpawnerUnavailable { channel: &'static str },
    SessionMeterUnavailable { channel: &'static str },
}
```

## Host Traits

The host contract is `crates/gproxy-core/src/host.rs`; `BindingStore`
lives in `crates/gproxy-channel-api/src/surface/state.rs` and
`ControlPlane` in `crates/gproxy-core/src/control.rs`. Every async method
returns the workspace `BoxFuture`: `Pin<Box<dyn Future<Output = T> + Send
+ 'a>>` on native, without `Send` on wasm. `MaybeSend` and `MaybeSync` are
`Send` and `Sync` on native and empty on wasm. Provided methods are shown
with `{ ... }`.

```rust
pub trait Host: MaybeSend + MaybeSync + 'static {
    type Credentials: CredentialStore;
    type Cache: CacheBackend;
    type Transport: UpstreamTransport;
    type Usage: UsageSink;
    type Capture: CaptureSink;

    fn credentials(&self) -> &Self::Credentials;
    fn cache(&self) -> &Self::Cache;
    fn transport(&self) -> &Self::Transport;
    fn usage(&self) -> &Self::Usage;
    fn capture(&self) -> &Self::Capture;
    fn authenticate<'a>(
        &'a self,
        request: &'a RequestCtx,
    ) -> BoxFuture<'a, Result<CallerIdentity, CoreError>>;
    fn admit<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        request: &'a RequestCtx,
        operation: Option<OperationKey>,
        plan: &'a Plan,
    ) -> BoxFuture<'a, Result<(), CoreError>>;
    fn finish_admission<'a>(
        &'a self,
        request_id: &'a str,
        settlement: Option<&'a Settlement>,
    ) -> BoxFuture<'a, ()>;
    fn admit_credential<'a>(
        &'a self,
        target: &'a crate::control::Target,
        body: &'a bytes::Bytes,
    ) -> BoxFuture<'a, Result<(), CoreError>>;
    fn count_tokens<'a>(
        &'a self,
        model: &'a str,
        body: &'a bytes::Bytes,
        tokenizer_map: Option<&'a serde_json::Value>,
    ) -> BoxFuture<'a, Result<u64, CoreError>>;
    fn record_credential_health<'a>(
        &'a self,
        credential: CredentialId,
        model: &'a str,
        credential_version: u64,
        health: CredentialHealth,
        response_status: Option<http::StatusCode>,
        detail: &'a str,
    ) -> BoxFuture<'a, ()>;
    fn observe_credential_quota<'a>(
        &'a self,
        credential: CredentialId,
        observations: Vec<gproxy_channel_api::QuotaObservation>,
    ) -> BoxFuture<'a, ()> { ... }
    fn wait<'a>(&'a self, duration: Duration) -> BoxFuture<'a, ()>;
    fn surface_usage<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        provider: &'a ProviderRef,
        credential: CredentialId,
    ) -> Box<dyn UsageView + 'a>;
    fn spawner(&self) -> Option<&dyn Spawner> { ... }
    fn bindings(&self) -> Option<&dyn BindingStore> { ... }
    fn oauth(&self) -> Option<&dyn gproxy_channel_api::OAuthService> { ... }
    fn continuations(&self) -> Option<&dyn ContinuationStore> { ... }
}
```

`Host` is the aggregate handed to `Core::new`; associated types keep every
service statically dispatched. `authenticate` maps the normalized request
to a `CallerIdentity` (user, key, organization, team) or returns
`CoreError::Unauthorized`. `admit` applies permissions, rate limits and
the quota pre-charge; when it returns an error it must leave no
reservation behind. `finish_admission` reconciles with `Some(settlement)`
or refunds with `None`. `admit_credential` is the per-credential RPM/TPM
gate. `count_tokens` is the host's tokenizer ladder. `wait` is the host's
timer for bounded service-surface polling; the core never picks an
executor. `surface_usage` lends a `UsageView` to service-surface
synthesizers. The four optional accessors default to `None`; the quota
observer defaults to dropping observations.

### `CredentialStore` (mandatory)

```rust
pub struct CredentialRecord {
    pub id: CredentialId,
    pub channel: String,
    pub kind: String,
    pub secret: serde_json::Value,
    pub version: u64,
}

pub trait CredentialStore {
    fn load<'a>(&'a self, id: CredentialId) -> BoxFuture<'a, Result<CredentialRecord, StoreError>>;

    fn persist_rotation<'a>(
        &'a self,
        id: CredentialId,
        secret: serde_json::Value,
        version: u64,
    ) -> BoxFuture<'a, Result<(), StoreError>>;

    fn lease_refresh<'a>(
        &'a self,
        id: CredentialId,
        ttl: Duration,
    ) -> BoxFuture<'a, Result<bool, StoreError>>;
}
```

The core receives decrypted secret material in the channel's documented
JSON shape; decryption is the store's concern and the core never sees a
cipher. `persist_rotation` must be atomic and guarded by `version`: a
stale version must fail rather than overwrite. Claude rotates the refresh
token on every refresh, so a lost write bricks the credential, which is
why this trait is not optional. `lease_refresh` returns whether the caller
holds the exclusive lease, so concurrent requests refresh once.

### `CacheBackend`

```rust
pub trait CacheBackend {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, StoreError>>;
    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), StoreError>>;
    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), StoreError>>;
    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, StoreError>>;
    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected_state: Vec<u8>,
        state: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, StoreError>>;
    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, StoreError>>;
}
```

`incr` starts an absent key at zero and sets its expiry from `ttl`;
incrementing an existing key never changes its expiry.
`compare_incr_and_set` must commit both writes or neither — quota
reconciliation depends on it. `compare_and_swap` backs long-lived
ownership leases. The cache must be shared by every instance that shares
a database; `gproxy-store` ships in-process, Redis, Upstash and libSQL
implementations.

### `UpstreamTransport`

```rust
pub trait UpstreamTransport: MaybeSync {
    fn send<'a>(
        &'a self,
        request: http::Request<bytes::Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<crate::boundary::ByteStream>, TransportError>>;

    fn open_websocket<'a>(
        &'a self,
        request: http::Request<bytes::Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>>;
}
```

Request bodies are buffered `Bytes` because transforms and retries need
replay; responses stream. `gproxy-upstream` is the canonical
implementation and an embedder may bring its own.

### `UsageSink` and `CaptureSink`

```rust
pub trait UsageSink {
    fn record<'a>(&'a self, settlement: &'a Settlement) -> BoxFuture<'a, ()>;
}

pub trait CaptureSink {
    fn record<'a>(&'a self, capture: &'a Capture) -> BoxFuture<'a, ()>;
}
```

The funnel offers every settlement and every captured exchange; the sink
decides retention and redaction. `gproxy-app` writes usage rows and wire
logs; an embedder may aggregate in memory or drop. Neither runs on the
hot path's critical section.

### `Spawner`

```rust
pub trait Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>);
    #[cfg(target_arch = "wasm32")]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()>>>);
}
```

Optional. If the host provides one, stream settlement detaches after the
response is done; if not, settlement completes inline before the stream
closes. A host without a spawner must keep polling an upstream stream
after the client disconnects and must close bridged websockets
explicitly, because `Drop` cannot await an asynchronous sink.

### `BindingStore`

```rust
pub trait BindingStore: MaybeSync {
    fn save<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
        credential: CredentialId,
        summary: Value,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    fn find<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<Binding>, StateError>>;
    fn delete<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    // ... and a paginated `list`
}
```

Durable resource-to-credential bindings for stateful service surfaces
and resource-affinity operations (files, videos, realtime calls). There
is no in-memory default on purpose: bindings must survive restarts and be
shared across instances. A host that returns `None` cannot register
channels that declare surface tables; `Core::new` fails loudly instead.

### `ControlPlane`

```rust
pub trait ControlPlane: gproxy_channel_api::MaybeSend + gproxy_channel_api::MaybeSync {
    fn resolve_alias(&self, model: &str, mode: &RoutingMode) -> String { ... }
    fn resolve(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError> { ... }
    fn resolve_variant(&self, model: &str, mode: &RoutingMode) -> Option<String> { ... }
    fn resolve_preprocessed(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError>;
    fn pricing(&self, provider: &ProviderRef, upstream_model: &str) -> Option<Pricing>;
    fn exposed_models(&self) -> Vec<ExposedModel>;
    fn provider_catalogue(&self) -> Vec<ExposedModel> { ... }
    fn detached(&self) -> Box<dyn ControlPlane>;
}

pub struct Plan {
    pub targets: Vec<Target>,
    pub budget: FailoverBudget,
}

pub struct Target {
    pub provider: ProviderRef,
    pub credential: CredentialId,
    pub upstream_model: String,
    pub tier: u32,
    pub rules: TargetRules,
}

pub struct FailoverBudget {
    pub max_attempts: u32,
}
```

The read model is synchronous by design: implementations answer from an
in-memory snapshot, never from I/O on the hot path. The provided
`resolve` applies `resolve_alias`, then `resolve_variant`, then
`resolve_preprocessed`. `gproxy-app` implements the trait over an ArcSwap
snapshot; an embedder may implement it over static configuration, or skip
it and build a `Plan` for `execute_planned`. `pricing` returning `None`
settles at zero cost with a warning rather than refusing the request.

`OAuthService` (issuer state for the emulated vendor-auth surfaces) and
`ContinuationStore` (process-local ownership of live upstream streams,
native-only) are optional; channels that need them are rejected by
`Core::new` when the host returns `None`.

## Boundary Types

From `crates/gproxy-core/src/boundary.rs` and `usage.rs`:

```rust
pub struct RequestCtx {
    pub request_id: String,
    pub client_ip: Option<std::net::IpAddr>,
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub upgrade: bool,
    pub mode: RoutingMode,
}

pub enum RoutingMode {
    Aggregated,
    Namespace { namespace: String },
    Scoped { provider: String },
    Named { name: String },
}

pub struct ExecOutcome {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ResponseBody,
    pub disposition: Disposition,
    // private settlement proof
}

pub enum ResponseBody {
    Full(Bytes),
    Stream(ByteStream),
    WebSocket(Box<dyn gproxy_channel_api::WsDuplex>),
}

pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, TransportError>> + Send>>;

pub struct Settlement {
    pub request_id: String,
    pub provider_id: i64,
    pub credential_id: crate::host::CredentialId,
    pub upstream_model: String,
    pub usage: NormalizedUsage,
    pub cost: Decimal,
    pub source: UsageSource,
    pub ended: Ended,
    pub latency_ms: u64,
}
```

`request_id` is host-assigned and threads every log line, capture row and
usage row. The body is buffered `Bytes`, so clones are free and retries
replay. `mode` comes from `gproxy_app::ingress::normalize_path` or from
your own prefix handling; `Named` is settled by the control plane into a
namespace, route or provider. `ExecOutcome` cannot be constructed outside
the core: the private settlement proof means every outcome a host renders
has passed through the funnel. `CoreError::status()` and
`CoreError::body_json()` give every host the same status and OpenAI-style
error envelope: 401 unauthorized, 403 forbidden, 404 unknown route or
provider, 400 unsupported, 429 rate limited, 402 quota exceeded, 502 for
no credentials, exhausted upstreams and transport failures, 500 for
transform, store and internal errors.

## wasm Notes

- `BoxFuture`, `ByteStream` and `Spawner::spawn` drop `Send` on `wasm32`;
  `MaybeSend`/`MaybeSync` become empty markers. One trait definition
  serves both targets.
- Shared handles are `Rc` on wasm and `Arc` on native.
- Without a `Spawner`, settlement runs inline before the response stream
  ends. The edge host keeps the stream alive through the platform's
  `waitUntil` continuation.
- Channels that require a `ContinuationStore` are native-only and rejected
  by `Core::new` on a host without one.
- Persistence on wasm is libSQL over fetch; the cache is the libSQL table
  or Upstash.

## The Reference Embedder: `gproxy-app`

```rust
impl App {
    pub async fn start(config: Config) -> Result<AppHandle, AppError>
}

impl AppHandle {
    pub async fn execute(
        &self,
        request: RequestCtx,
    ) -> Result<ExecOutcome, gproxy_core::CoreError>
    pub async fn admin_dispatch(
        &self,
        parts: &http::request::Parts,
        body: bytes::Bytes,
    ) -> Option<http::Response<bytes::Bytes>>
    pub async fn portal_dispatch(
        &self,
        parts: &http::request::Parts,
        body: bytes::Bytes,
    ) -> Option<http::Response<bytes::Bytes>>
    pub async fn mutate(
        &self,
        mutation: crate::ControlMutation,
    ) -> Result<crate::MutationResult, AppError>
    pub async fn reload(&self) -> Result<(), AppError>
    pub fn shutdown(&self)
    pub async fn wait_shutdown(&self)
}
```

`App::start` opens the store and migrates, seeds the embedded price
catalog on a fresh store, prepares the master-key cipher (running a
rotation when armed), seeds the first-run administrator, builds the
snapshot control plane, cache, transport and tokenizer registry, calls
`Core::new(host, channels)`, and schedules the cleanup sweep. `Config`
comes from `Config::from_env()` or `NativeCommand::from_env()` on native,
from `Config::libsql(url, auth_token, MasterKeyConfig)` on wasm, or from
`Config::sqlite(listen_addr, data_dir, MasterKeyConfig)` in code.
`gproxy_app::ingress::decode_body` and `normalize_path` are public so a
host can apply the same body decoding and named-prefix handling. The two
hosts are `gproxy_host_axum::AxumServer::bind_with_config(app, address,
HostConfig)` and `gproxy_host_edge::start(EdgeConfig)`.

## Libraries Usable Alone

**`gproxy-protocol`**: `match_ingress_for(method, path, preferred)`,
`Operation::spec()`, `registered_operations()`, and the wire models under
`openai`, `claude`, `gemini` and `aws`. Every model keeps unmodeled fields
in a flattened `rest`, so a round trip never drops what an upstream
added. Enums are `#[non_exhaustive]` for external consumers; the
`exhaustive` feature turns a new variant into a compile-error checklist.

**`gproxy-transform`**:

```rust
pub fn can_transform(source: OperationKey, target: OperationKey) -> bool

pub fn request(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
    upstream_model: &str,
    stream: bool,
) -> Result<Bytes, TransformError>

pub fn response(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
) -> Result<Bytes, TransformError>

pub fn response_stream(
    source: OperationKey,
    target: OperationKey,
) -> Result<ResponseStream, TransformError>

pub fn response_stream_framed(
    source: OperationKey,
    target: OperationKey,
    source_framing: StreamFraming,
    target_framing: StreamFraming,
) -> Result<ResponseStream, TransformError>
```

Pairs are declared explicitly, there is no intermediate format: Chat ↔
Claude, Responses ↔ Claude, Claude ↔ Gemini, Gemini ↔ Chat, Gemini ↔
Responses, Chat ↔ Responses, models and count-tokens OpenAI ↔ Claude, and
Compact → Claude, plus envelope promotions such as Responses over
WebSocket. Everything is synchronous and I/O-free.

**`gproxy-tokenize`**: `count(model, body, map, registry) -> u64`,
`count_detailed`, `count_text`, `try_count` and `harvest`, behind the
features `tiktoken`, `hf-registry` and `bundled-fallback`. See
[Pricing & Tiers](/reference/pricing/) for the ladder it implements.
