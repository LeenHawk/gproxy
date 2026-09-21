---
title: "Embedding the Core"
description: "gproxy-sdk is the embeddable handle: assembly, management writes, login, resolution, calling, queries and synchronization — with no server and no identity in it."
---

`gproxy-sdk` is the embeddable GPROXY handle. It is what the gateway itself is
built on, and an application can link it directly.

**What it sells is not an HTTP client — it is a call with pooled-credential
discipline:** a credential pool, automatic refresh, failover between providers,
and metered settlement. That is also why the database is not optional inside
it. Claude rotates its refresh token on every refresh, so a handle that kept
credentials in memory would kill the credential on the first refresh.

Nothing in the workspace is published to a registry. Embedding means a git or
path dependency.

```toml
[dependencies]
gproxy-sdk = { git = "https://github.com/LeenHawk/gproxy", branch = "4.0" }
```

The workspace is Rust edition 2024 at `4.0.0-dev`. **The public surface is not
stable.**

## Assembling One

```rust
use gproxy_sdk::{GproxyBuilder, SyncMode};

let gproxy = GproxyBuilder::sqlite("gproxy.db")
    .await?
    .master_key([0u8; 32])
    .sync_mode(SyncMode::Background)
    .build()
    .await?;

for channel in gproxy.channels() {
    println!("{} ({})", channel.display_name, channel.id);
}
println!("serving revision {}", gproxy.revision().0);
```

`build()` opens nothing it was not given. It synchronizes the entity schema
(unless told not to), creates the settings row, assembles the engine, loads the
first snapshot, and — in background mode — starts the subscription and poll
loops.

**A secret codec is never chosen implicitly.** `master_key` seals with
AES-256-GCM, `plaintext_secrets` is the explicit opt-out, and a build with
neither is refused. There is no quiet default that turns out to have been
plaintext.

## Features

| Feature | Effect |
| --- | --- |
| one channel's name (`codex`, `kiro`, `openai`, …) | compile that channel in and register it |
| `channels` | every channel this workspace ships |
| `postgres` / `mysql` | extra drivers, native only. SQLite is always available natively |
| `libsql` | libSQL/Turso over the Hrana HTTP pipeline, **every target** |
| `d1` | a marker for a Cloudflare D1 binding |
| `memory` *(default)* | the process-local cache, the default natively |
| `redis` | for more than one instance |
| `fs` *(default)* | local file storage, native only |
| `s3` | S3/R2 storage |
| `bundled-vocabulary` *(default)* | DeepSeek's vocabulary for token estimation |
| `ts` | `ts-rs` declarations for every DTO and every channel descriptor |

The default set compiles for `wasm32-unknown-unknown`, and so does `libsql`.

Native builds use the `reqwest` transport. A host that wants another backend —
`wreq` for TLS emulation, a native stack for the Codex CLI's fingerprint —
enables that feature on the client crate, or hands the builder a client pool of
its own.

## Calling

```rust
let execution = gproxy
    .call(
        OperationKey { operation: Operation::GenerateContent, dialect: Dialect::OpenAi },
        request,
    )
    .scope("user:u-1")
    .attribution(UsageAttribution { user_id: Some("u-1".into()), ..Default::default() })
    .budgets(vec![BudgetOwner::new("user", "u-1")])
    .credentials(["cred-1".to_owned()].into())
    .send()
    .await?;

let (response, usage) = execution.into_parts();
```

`call` builds the request; `send` resolves the model name and walks the plan.

**A scope is required and there is no safe default for it.** It is the
isolation boundary the engine keeps one caller's credential affinity inside, so
a default would mean strangers sharing continuations.

`connect` is the same builder over a WebSocket handshake.

`.channel("claudecode")` narrows the candidate set. That is narrowing, not
choosing a provider: a channel is a real difference to a caller —
`claudecode` spends an OAuth subscription allowance while `claudeapi` spends an
API key per token, with independent quota pools and billing — while a provider
is a configuration row nobody outside the instance should have to know. Pinning
a channel leaves the handle plenty to do: choose among that channel's
providers, choose among a provider's credentials, and move on inside the
channel when one fails.

Everything the application layer decided — the allowed providers and
credentials, the budget chain, the session — is **passed in**. Nothing here
authenticates anybody.

## What Is Not Here

- **Identity.** Users, API keys, organizations, teams, permissions,
  subscriptions, rate limits and the OAuth issuer belong to `gproxy-app`, which
  reads the same durable revision from the same batch.
- **Downstream authentication.** Nothing here decides who a caller is.
- **A server.** No listener, no router, no middleware, no CLI.
- **A console.** This crate generates the TypeScript types a UI is written
  against; the UI is the application's.
- **Execution.** Attempts, conversion, rewriting, observation, budgets and
  settlement are the engine's; this crate decides only what to hand it.

## Management

`gproxy.manage()` is the write side: one accessor per configuration family, all
over the same commit primitive.

| Family | Rows | Beyond CRUD |
| --- | --- | --- |
| `providers()` | `providers` | `reset_routing_defaults` |
| `credentials()` | `credentials` | `reveal_secret`, `set_status`, `refresh`, `quota_probe`, `quota_read`, `quota_reset`, `health_reset`, `limit_status` |
| `models()` / `provider_models()` | the catalogue | |
| `routes()` / `route_members()` / `exposed_models()` | routing | |
| `connection_profiles()` | outbound stacks | |
| `settings()` | the one settings row | `get` / `update` only |
| `rewrite()` | rule sets, rules, attachments | `replace_rules` |
| `endpoints()` | `operation_rules`, `operation_endpoints` | |
| `quotas()` | `quotas` | `budget_status`, `reset_budget`, `limit_status`, `reset_limit` |
| `pricing()` | rules, rates, tiers | |
| `transfer()` | everything configured | `export`, `import` |
| `catalog()` | nothing, until applied | `channels`, `default_models`, `apply_default_prices`, `tls_presets`, `rule_presets` |
| `connectivity()` | `provider_models` | `test`, `model_test`, `discover_models`, `apply_discovered` |
| `tokenizer()` | vocabulary files | `vocabularies`, `fetch`, `progress`, `delete`, `auth` |

The first ten are ordinary CRUD plus a batch. Ids are caller-supplied when
given and minted otherwise, timestamps are Unix milliseconds, decimals travel
as strings, and a credential's secret is never in a DTO.

A write names its scopes, and that is the only thing the caller chooses. A
credential-state scope takes the cheap reload path, and it is valid **only**
for a write limited to the secret, its expiry and the lifecycle status —
everything else about a credential is frozen into the execution snapshot at
assembly and needs a full reload.

## Queries

`gproxy.query()` is the read side. Nothing here writes, so nothing here moves
the revision.

| Family | Answers |
| --- | --- |
| `usage()` | `records`, `summary`, `group`, `trend` |
| `quota()` | `windows`, `settlements`, `credential_cycles`, `counted_windows`, `budget_status` |
| `logs()` | `list`, `detail(request_id)` |

**Aggregation happens in Rust, over a scan cap.** The usage metrics column is
one JSON document per request and no supported backend can sum inside it, so
`summary`, `group` and `trend` read the matching rows and fold them in
key-ordered chunks. Every aggregate takes a scan budget, defaulting to and
clamped by 50,000 rows, and an aggregate that reached it comes back
`truncated: true` with the scanned count — never a smaller number presented as
the whole truth. A trend is bounded a second way: a zero or negative bucket, a
backwards range, and a range producing more than 5,000 buckets are all refused.

Two numbers do not come out of the document. Cost is read from the indexed
column written once at settlement, and the currency is `None` when the scanned
records disagreed about it, because summing dollars and euros is not a total.

**Management lists page by offset; the request log pages by cursor.** That is
not a style choice: a management list is a bounded set a person pages through,
while a request log is an append-only stream whose head keeps moving as it is
read, and an offset page over that repeats or skips rows. The log cursor is two
halves — a timestamp and the row id — because two requests can start in the
same millisecond, and a timestamp-only cursor either loops on that pair forever
or skips past it.

**Nothing in `logs()` redacts.** The observer applied the deployment's policy
as it *wrote* these rows, so what is stored is already what may be shown. A
host must not assume a second pass happens on read: if a secret is in the
database, the policy allowed it there, and filtering on read could not undo
that.

## Synchronization

| | Carries | Fails by |
| --- | --- | --- |
| an invalidation on the shared cache | "look again", within milliseconds | losing messages |
| the `settings.config_revision` poll | the durable truth, every 30 s | being slow |

On wasm the cache defaults to the database-backed one and synchronization is
always **manual**: an isolate does not outlive its request, so there is no
background loop to run and `tick()` at the top of a request is the whole
mechanism.

```rust
// tick() = drain the delivered notifications, then poll the revision once.
instance.tick().await;
```

Draining is cheap enough to do on every request; the poll is not, so a host
that wants only the cheap half can call it alone. Manual mode establishes the
subscription at build time and discards the leading resync hint — the first
load *is* that resync — so a write between assembly and the first tick is not
missed by both mechanisms at once.

## Type Export

```sh
GPROXY_TS_OUT=console/src/generated \
  cargo test -p gproxy-sdk --features ts export_types
```

Without the variable the test returns immediately and writes nothing, so
`cargo test --all-features` stays hermetic and a generated directory is only
ever rewritten on purpose. With it, the directory is **wiped first** — a stale
declaration for a DTO that no longer exists would keep type-checking long after
Rust dropped it — and an index re-exporting everything is written last.

The channel catalogue is in there too. A console renders a provider form from
the descriptor the channel itself returns, so it should be typed by that
descriptor rather than by a copy that drifts the next time a channel adds a
configuration key.

The export list is hand-written, because Rust cannot enumerate a module's types
at run time. That list is exactly what drifted in v3 — a DTO was added, nobody
remembered the list, and the console silently went without a type for it — so a
second test reads the module's own source and compares its re-exports against
the list. **Adding a DTO and forgetting the list is a red test, not a missing
file.**

## Above the SDK

An application that also wants identity links `gproxy-app`, which adds
authentication, admission, the admin and portal operation families and the
OAuth issuer, and owns **no** transport. `gproxy-host-axum` turns those into an
HTTP surface, `gproxy-host-edge` mounts the same router in a Worker, and
`gproxy-host-tauri` binds the management surfaces to IPC.

Each of those seams is a place an embedder can enter. See
[Architecture](/introduction/architecture/).
