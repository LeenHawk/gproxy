# gproxy-core

English | [简体中文](README.zh-CN.md)

Provider execution data contracts. The upper layer resolves routes/model aliases,
authenticates callers, evaluates policies and admits requests before calling core.
This crate currently supplies data structures and monotonic in-memory publication;
request execution and credential selection are not wired yet.

| Module | Structures |
|---|---|
| `data` | Provider/credential execution snapshot, channel/client handles, prepared rewrite matchers |
| `runtime` | Atomic credential material, credential affinity/rotation, health and execution-data invalidation |
| `context` | Resolved execution target, opaque caller scope/session, request/attempt/exchange, usage reports |

`CoreData` contains providers, credentials and reusable rewrite sets. It does not
contain routes, exposed-model aliases, identity tables, permissions, OAuth client
allowlists, subscriptions, pricing or admission rules. These belong to the upper
layer, along with route affinity and cross-provider balancing/failover.

`ExecutionTarget` supplies one selected Provider, an optional resolved upstream
model and the exact permitted credential set. Future execution must rotate/retry
only within that set; an empty set must never fall back to the global provider
pool. The upper layer supplies the final attempt budget and an authenticated,
opaque isolation scope. Core does not interpret user/key roles. Bound remote
resources must be passed with their original target and restricted credentials;
assignment references are attribution, not permission to move a resource.

`CredentialStrategy::RoundRobinAffinity` serializes as `round_robin_affinity`.
New/unbound sessions receive credentials in round-robin order; subsequent requests
reuse the same eligible credential without advancing the rotation cursor. A missing,
expired or unusable/disallowed pin triggers reassignment within the supplied set.
Without a stable session identity, use ordinary per-request round-robin. Scope pins
by the caller isolation scope and Provider; do not share them across callers.
Concurrent first requests for the same session must converge on one binding. This
is a selection contract only: persistence and the selector are still pending.

`Core<C>` owns an `Arc<Store<C>>`, injected `Arc<dyn Cache>`, and `ArcSwap<CoreData>`.
Construction does no I/O. `publish_snapshot` accepts only a newer revision of an
already-validated execution snapshot. Requests pin their own Arc. Durable revision
allocation, data assembly and notification handling remain upper-layer integration.

`CredentialData` holds execution configuration, resolved client and a shared
`CredentialState`. Secret, expiry and Store version publish atomically; each
attempt pins one version. Publish only after Store CAS or authoritative loading.
Equal/older versions are rejected. Reuse a slot for a still-live credential, retire
it on deletion, and create a new slot for a recreated credential. There is no
implicit refresh lease or persistence in the publication method.

CoreData and decrypted material are not Debug/Serialize. Session identity is
provided by ingress; RequestFallback is not cross-request stable. Protocol request,
response and stream types remain in gproxy-protocol. `NormalizedUsage` is reused
from gproxy-channel: one request can have multiple attempts, each with multiple
physical exchanges. Capture IDs belong to physical exchanges, not stream chunks.
`UsageReport` returns observed usage to upper-layer logging/pricing/settlement;
it contains no charges, subscription allocations or admission reservations.

`RewriteTarget` represents Body (owning optional JSON paths), Header (a typed
HeaderName), or Query (a decoded parameter name). Header/Query operate on repeated
values without flattening them; Query is request-only. Compiler validation and
actual mutations are pending; see [rewrite design](../../design/core-rewrite.md).

ProviderData also carries `operation_urls`, keyed by the destination
`OperationKey` and HTTP/WS transport. `operation_url` looks up the enabled,
validated override prepared from Store OperationEndpoint rows; None means use
provider/channel defaults. Applying the URL during channel execution remains
pending with the other execution prototypes.

## Function prototypes

[`src/api.rs`](src/api.rs) defines the next implementation surface. All newly
introduced functions currently return `CoreError::NotImplemented`, with no
outbound calls, cache writes or credential refresh. They are reviewable signatures,
not a functional execution pipeline.

| Entry point | Contract |
|---|---|
| `send` | Already-routed HTTP invocation, including streaming responses |
| `connect` | Already-routed WS invocation, preserving rejected HTTP responses |
| `select_credential` | Select only from the supplied permitted set, with attempted IDs excluded |
| `refresh_credential` | Shared lease, durable full-replacement CAS, then publication |
| `rewrite_request` | Body/Header/Query before channel shaping |
| `rewrite_response` | Body/Header after original usage observation |
| `record_outcome` | Credential health/affinity from a completed attempt |
| `compile_rewrite_rule` | Free function for validating/preparing one persisted rule |

`Execution<T>` retains the exact protocol transport result plus `UsageCompletion`.
The completion future is distinct from response headers and must eventually finish
when the stream/socket finishes or is dropped. `RewriteLimits` requires a positive
per-unit byte limit. These lifecycle contracts are not implemented by the stubs.
Prototype validation is native/WASM Clippy; no pretend upstream-execution tests.

```sh
cargo test -p gproxy-core
cargo clippy -p gproxy-core --all-targets -- -D warnings
cargo clippy -p gproxy-core --target wasm32-unknown-unknown --all-targets -- -D warnings
```

Tests cover concurrent publication and retained in-flight views. They do not
establish a working selector or gateway. See [crate boundaries](../../design/crates.md).
