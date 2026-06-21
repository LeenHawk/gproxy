# gproxy v2 架构

v2 是一次重写，不再沿用 v1 的多 crate / SDK 分发结构。当前仓库是一个 Rust crate，
同时产出 native binary 和 wasm library；React console 是独立的 `pnpm` 应用，构建后嵌入
native 二进制或作为静态资源部署。

这页描述当前代码的边界，不再记录早期实施计划。

## 核心决策

| 决策 | 当前形态 |
| --- | --- |
| 单 crate | `Cargo.toml` 定义一个 `gproxy` crate，含 `src/lib.rs` 和 `src/main.rs`。 |
| Operation 优先 | 协议能力按 `Operation` / `OperationGroup` 组织，而不是按 OpenAI/Claude/Gemini 家族分组。 |
| 分层 pipeline | 请求经过 classify、auth、preprocess、route、balance、transform、process、channel、settle。 |
| channel 只接上游 | channel 负责 auth、endpoint、响应分类和少量整形，不负责协议转换或规则改写。 |
| 控制面快照 | provider、route、rule、user 等读多写少数据进入 `ControlPlaneSnapshot`，请求热路径读快照。 |
| 两类后端 | `CacheBackend` 处理 session、限流、配额预扣、失效广播；`PersistenceBackend` 处理控制面和日志/用量真相源。 |
| native + edge | native 使用 Axum + wreq；edge 使用 WinterCG fetch + libSQL/Turso + Upstash/libSQL cache。 |

## 目录结构

```text
src/
|-- main.rs              # native CLI/env, persistence/cache/upstream 装配, serve
|-- lib.rs               # shared module surface, wasm export surface
|-- config/              # RuntimeConfig, feature-selected backend config
|-- app/                 # AppState, bootstrap, snapshot, import/export, v1 migration
|-- protocol/            # Operation taxonomy and provider wire types
|-- transform/           # provider-to-provider protocol transforms by operation
|-- process/             # provider rule-set mutations after transform
|-- channel/             # upstream adapters and channel registry
|-- pipeline/            # request lifecycle orchestration
|-- http/
|   |-- server/          # native Axum routers and handlers
|   |-- client/          # upstream transport abstraction: wreq or fetch
|   |-- admin_api/       # edge/test dispatcher for admin and portal APIs
|   `-- edge/            # wasm fetch entry
|-- store/
|   |-- cache/           # memory, redis, libSQL cache, Upstash cache
|   `-- persistence/     # file, db, libSQL persistence
|-- admin/               # session, CSRF, auth guards
|-- billing/             # price resolution, pending quota accounting
|-- credentials/         # refresh, upstream model pull, usage endpoints
|-- health/              # passive breaker/cooldown/latency soft state
|-- tokenize/            # local token counting and tokenizer registry
|-- selfupdate/          # native self-update implementation
`-- usage/               # usage extraction helpers
```

## Operation Taxonomy

`src/protocol/operation.rs` is the protocol taxonomy center. The code models
capabilities first:

- `OperationGroup`: `Models`, `CountTokens`, `GenerateContent`, `Images`,
  `Embeddings`, `Compact`, `Conversation`.
- `Operation`: concrete operations such as `ListModels`, `GenerateContent`,
  `StreamGenerateContent`, `CreateEmbedding`, `CompactContent`.
- `OperationKind`: provider wire shape for that operation.
- `OperationKey`: `(operation, kind)` pair used by routing rules and transforms.

Content generation has four wire kinds because OpenAI has two distinct native
formats:

- `OpenAiResponses`
- `OpenAiChatCompletions`
- `ClaudeMessages`
- `GeminiGenerateContent`

Non-content operations use the coarser provider family: OpenAI, Claude, Gemini.
This keeps the matrix capability-oriented and avoids provider-family buckets
leaking into transform design.

## Request Lifecycle

At a high level, one gateway request moves through this path:

```text
HTTP request
  -> ingress/classify
  -> auth user API key
  -> preprocess model name
  -> route or scoped provider resolution
  -> permission, rate-limit, quota admission
  -> balance route member and credential
  -> transform into provider-native protocol if needed
  -> process provider rule sets
  -> channel prepare/auth/endpoint
  -> upstream client send
  -> channel classify response
  -> failover or settle
  -> response shaping and protocol transform back
  -> logs, usage, quota reconciliation
```

The main orchestrator is `pipeline::execute`. Supporting modules keep the
phases small:

| Module | Responsibility |
| --- | --- |
| `pipeline/classify.rs` | Work out the inbound operation and wire kind. |
| `pipeline/auth.rs` | Resolve user identity from API key. |
| `pipeline/preprocess.rs` | Normalize model names and aliases. |
| `pipeline/route.rs` | Resolve aggregated route or scoped provider. |
| `pipeline/authz.rs` | Enforce route permissions, rate limits, and quotas. |
| `pipeline/balance/` | Pick route member and credential candidates. |
| `pipeline/transform.rs` | Dispatch protocol transforms. |
| `pipeline/failover/` | Retry/cooldown/AuthDead flow around upstream send. |
| `pipeline/settle/` | Persist successful usage and reconcile quota deltas. |
| `pipeline/local_ops.rs` | Local model/count-token operations that do not call upstream. |

Same-protocol requests can stay on the minimal parsing path. Cross-protocol
requests enter `transform/`, then `process/`, then the selected channel.

## Routing Model

Aggregated routes and scoped routes are intentionally different:

| Mode | Path shape | Resolution |
| --- | --- | --- |
| Aggregated | `/v1/*`, `/v1beta/*` | model name resolves to route, then route member, then credential. |
| Scoped | `/{provider}/v1/*`, `/{provider}/v1beta/*` | URL chooses provider; model goes directly to that provider. |

Logical routing has three layers:

1. Alias/preprocess: external names map to canonical route names.
2. Route: a logical model points to one or more route members.
3. Member/credential: route member selects provider/upstream model; provider
   credential strategy selects the actual credential.

Permissions are checked against the exposed route/provider name, not against a
hidden upstream credential. This is what lets one route hide multiple upstream
models behind one user-facing model name.

## Transform, Process, Channel

These three layers are deliberately separate.

`transform/` owns protocol conversion. It is organized by operation, with shared
helpers under `transform/common/` and operation-specific modules like
`generate_content`, `count_tokens`, `models`, `images`, and `embeddings`.

`process/` applies provider rule sets after transform and before channel
prepare. It operates on provider-native headers/body and applies rules in a
fixed order:

```text
system_text -> cache_breakpoint -> rewrite -> sanitize -> header
```

Bad or non-applicable process rules warn and skip; a bad rule should not break
traffic.

`channel/` is pure upstream access. A `Channel` declares:

- stable channel id;
- provider family;
- routing table;
- request preparation;
- response disposition;
- optional request/response body shaping;
- optional stream decoder;
- optional OAuth refresh and usage endpoint support;
- optional native TLS/HTTP2 impersonation profile.

It does not own cross-protocol transforms, provider rule-set processing,
pricing, or token counting.

## AppState

Every request receives a clone of `AppState`, whose fields are all cheap
`Arc` handles:

| Field | Purpose |
| --- | --- |
| `config` | Immutable process runtime config from CLI/env. |
| `cache` | Session, rate-limit/quota counters, invalidation, refresh locks. |
| `persistence` | Durable control plane, logs, usage, audit, rollups. |
| `upstream` | Default upstream HTTP client. |
| `snapshot` | `ArcSwap<ControlPlaneSnapshot>` for hot-path reads. |
| `channels` | Channel registry keyed by channel id. |
| `cipher` | Secret sealing/opening via `GPROXY_MASTER_KEY` or plaintext mode. |
| `health` | Per-instance breaker/cooldown/latency soft state. |
| `refresh` | Per-credential single-flight OAuth refresh coordinator. |
| `client_pool` | Native wreq client pool keyed by proxy/fingerprint. |
| `tokenizers` | Local tokenizer registry when `count-local` is enabled. |
| `update_status` | Native self-update state. |

Control-plane mutations write persistence, reload the local snapshot, and
broadcast invalidation through the cache backend. Redis subscribers rebuild
their snapshot; edge isolates poll a shared config-version key because they do
not hold a pub/sub connection.

## Storage Backends

There are only two storage traits.

`CacheBackend` is for ephemeral/shared counters and coordination:

- `get`, `set`, `delete`;
- atomic `incr`;
- invalidation `publish` / `subscribe`;
- best-effort distributed lock for refresh single-flight.

Native implementations are memory and Redis. Edge implementations are libSQL KV
and Upstash Redis REST.

`PersistenceBackend` is the typed durable store. It owns provider, credential,
route, rule, identity, authz, usage, log, audit, and metrics operations. Native
implementations are file and db; edge implementation is libSQL/Turso over Hrana
HTTP.

Domain code should depend on these traits and record types, not on SeaORM,
filesystem layout, Redis, or libSQL details.

## Native Runtime

The native binary in `src/main.rs` does this order:

1. Parse CLI/env with clap.
2. Dispatch self-contained subcommands such as `update` and `migrate-v1`.
3. Build runtime config, cache config, persistence config, and upstream config.
4. Build `SecretCipher` from `GPROXY_MASTER_KEY`.
5. Run v1 SQLite migration on boot when enabled and applicable.
6. Open persistence and cache.
7. Optionally import first-boot bundle from `GPROXY_IMPORT_FILE`.
8. Ensure/recover admin user.
9. Build `ControlPlaneSnapshot`.
10. Build `AppState`, spawn invalidation/retention workers, and serve Axum.

The native HTTP surface is under `http/server/`:

- console static assets at `/console`;
- admin and portal API at `/admin/*` and `/user/*`;
- gateway routes at `/v1/*`, `/v1beta/*`, and scoped provider paths;
- admin-gated ops endpoints `/healthz`, `/version`, `/metrics`.

## Edge Runtime

The wasm entry in `http/edge/` does not run Axum. It exposes:

- `init(turso_url, turso_token, upstash_url, upstash_token, master_key)`;
- `fetch(request)`.

`init` opens libSQL/Turso persistence, chooses Upstash or libSQL cache, builds a
snapshot, channel registry, fetch upstream client, and cipher. `fetch` then:

1. refreshes the snapshot lazily when the config-version key changes;
2. enforces body limits;
3. handles admin-gated ops endpoints;
4. dispatches `/admin/*` and `/user/*` through `http/admin_api`;
5. sends gateway traffic through the same `pipeline::execute` core as native.

Edge deployments cannot use local TCP databases, local filesystem persistence,
native self-update, or native wreq TLS impersonation. They use Turso/libSQL,
Upstash/libSQL cache, and platform fetch.

## Security

Secrets are sealed through `crypto::SecretCipher`.

- `GPROXY_MASTER_KEY` set: standard base64 32-byte local KEK; secrets are stored
  as envelopes.
- `GPROXY_MASTER_KEY` absent: plaintext mode, with startup/runtime warnings.
- Existing plaintext rows remain readable for compatibility.
- A sealed row cannot be opened in plaintext mode.

Admin login uses Argon2 password hashes and opaque server-side sessions stored
in cache. API users use `user_keys` with digests for lookup. Console sessions
are httpOnly cookies; local insecure cookies require `GPROXY_INSECURE_COOKIES=1`.

CSRF checks are same-origin by default. `GPROXY_CORS_ORIGINS` enables explicit
credentialed CORS and cross-site session cookies for approved origins.

## Observability

Every gateway request gets a generated request id. Usage, downstream logs, and
upstream logs carry that id so a call can be joined across records.

`/metrics` is admin-gated and rendered from persisted aggregate data, not from
process-local counters. This keeps multi-instance and edge metrics tied to the
shared store.

Request/response body logging is controlled by instance settings. Secret headers
and known key fields are redacted unless redaction is explicitly disabled for
debugging.

## Billing and Quota

Billing resolves model pricing from provider model configuration and settles
only successful attempts. Failed failover attempts are logged but not billed.

Quota admission happens before upstream work using cache counters/pending state.
After a successful response, settlement writes durable usage, rollups, and quota
cost deltas. The durable quota update path is idempotent around request ids and
uses backend-specific atomicity/transaction behavior.

## Import, Export, and v1 Migration

`app/import.rs` and `app/export.rs` define a backend-independent control-plane
bundle. Import seals plaintext secrets with the target cipher; export opens
secrets and writes a plaintext bundle, so exported files must be protected.

`GPROXY_IMPORT_FILE` can seed an empty store on first boot.

The temporary `migrate-v1` feature reads a legacy v1 SQLite database, maps
control-plane rows into the v2 bundle shape, and imports them into v2. The boot
hook only runs before persistence opens the default SQLite file. See
`docs/v1-to-v2-migration.md`.

## Self-Update

Native self-update lives under `src/selfupdate/` and is intentionally
native-only. It checks signed release manifests, downloads artifacts, verifies
hash/signature, stages replacement, and exposes admin endpoints for check,
status, and apply.

Edge deployments do not self-update; they are updated by platform deployment.

## Build Features

Important feature sets:

| Feature | Effect |
| --- | --- |
| `default` | `cache-memory`, `persist-db`, `persist-file`, `upstream-wreq`, `count-local`, `migrate-v1`. |
| `full` | Default native set plus `cache-redis`. |
| `edge` | `cache-libsql`, `cache-upstash`, `persist-libsql`, `upstream-fetch`. |
| `migrate-v1` | Temporary v1 SQLite migration path. |

Native default persistence kind is `db`; without `GPROXY_DSN`, it derives a
SQLite DSN at `<data_dir>/gproxy.db`. Edge builds must use:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
```

## Code Rules

Project rules live in `CLAUDE.md` and apply to architecture work:

- keep files small and split by responsibility;
- search for existing modules before adding a new abstraction;
- avoid TDD-heavy workflows for this project;
- add focused tests for tricky logic and real regressions;
- run `cargo fmt` and `cargo clippy` for backend changes.
