# gproxy-app

English | [简体中文](README.zh-CN.md)

The GPROXY v4 product layer: who is calling, what they are allowed to do, and
the operations that change either. It owns identity (users, gateway API keys,
organizations, teams, permissions, subscriptions, rate limits, the OAuth
issuer, audit), admission, and the typed admin/portal/issuer operations.

It owns no server. There is no HTTP framework here, no router, no runtime, no
CLI, and none of the engine: routing, credential selection, failover, protocol
conversion, settlement and capture belong to `gproxy-core` and the handle above
it. That line is what lets the same decisions run behind a native axum server
and inside a Cloudflare Worker — the crate builds for `wasm32-unknown-unknown`,
and it stays that way because it has nothing platform-shaped to lose.

## The shape of a request

```text
transport (host)
  → auth        Caller   { user, role, api key, organization, team, subscription, grant }
  → admission   Admitted { allowed providers, allowed credentials, budget owners,
                           scope, session identity, rate-limit permit }
  → App::call   the engine executes with exactly those, and nothing re-derives them
```

Each step narrows, and nothing below re-opens what was narrowed above: the
engine is handed a provider set and a credential set, not a caller to
interpret.

`App` is where the three pieces meet — the engine handle, the instance
configuration and the identity snapshot — and it is the only thing a host
holds:

```rust,ignore
let app = App::new(gproxy, config);
app.reload_all().await?;                       // both snapshots, one startup

let data = app.data();                         // once per request, held throughout
let caller = app.authenticator(&data).authenticate_request(&headers).await?;
let outcome = app.call(&caller, request).await?;
//   outcome.execution  → the response to stream back
//   outcome.admitted   → the charges; keep it alive until the stream ends
```

`authenticator` and `admission` take the snapshot rather than loading it
because they borrow it for as long as they live and the published snapshot can
be replaced at any moment — which is also the request-scoped rule written down:
one load, one request.

## What exists now

| Module | Holds |
|---|---|
| `config` | `AppConfig`: plain serde, shared by both hosts, no filesystem or environment access |
| `snapshot` | `AppData`, the compiled identity of one configuration revision, and `AppSnapshot`, its monotonic publication |
| `auth` | the three ways a request names a caller, and the `Caller` all three produce |
| `admission` | `Caller` → `Admitted`: providers, credentials, budget owners, scope, session, rate-limit charges |
| `call` | `DataPlaneRequest` → `CallOutcome`: admit, then run the engine with exactly what was admitted |
| `service` | vendor CLI services: which credentials are in the target, and what role the caller has over them |
| `capture` | the downstream `capture_records` row and the `capture_links` edges to the upstream attempts core recorded |
| `publication` | `AppPublicationUrl`, plus the read and delete behind the host's download route |
| `operations` | the identity write families, each one revision commit plus a peer notification |
| `operations::portal` | the end user's self-serve surface, scoped to one `Caller` by construction |
| `operations::issuer` | the OAuth authorization server this instance runs **for downstream clients** |
| `dto` | the wire shapes those families exchange: string ids, millisecond timestamps, camelCase, no secrets |
| `audit` | the append-only trail, written outside the revision batch, with its redaction rule |
| `error` | `AppError`, with `status_code()` and a stable `code()` for the API envelope |

Every module in the table is implemented. What the crate still lacks is a host:
nothing here binds a transport, which is what `gproxy-host-axum` and
`gproxy-host-edge` are for.

## The snapshot

`AppData` is to this crate what `CoreData` is to `gproxy-core`, and both are
assembled from the same read: `Store::load_all_data()` returns control, routing
and identity rows in one batch, so the engine and the product layer are never a
revision apart in the middle of a request. A request takes one
`AppSnapshot::load()` and holds that `Arc` for its whole life.

| Index | Answers |
|---|---|
| `ApiKeyIndex` | digest → `ApiKeyIdentity` (key, user, role, organization, team, subscription, kind, expiry) |
| `MembershipIndex` | user → organizations and teams with roles, plus each team's parent organization |
| `CredentialOwnership` | credential → `Shared` / `User` / `Team` / `Org`, and whether a caller may select it |
| `PermissionSet` | `(subject, provider, model, operation)` → `Allow` / `Deny(reason)`, and the allowed provider set |
| `ClientAllowlist` | whether a user may authorize a given OAuth client |

alongside `users`, `organizations`, `teams`, `rate_limits`, `subscriptions`,
`plans`, `plan_limits`, `pools`, `pool_members` and `oauth_clients` as maps.

`publish_if_newer` is monotonic by revision, the same contract as
`Core::publish_snapshot`: reloads race — a poll and a notification fire for the
same write, and a slow load started first can finish last — and a snapshot
going backwards would resurrect deleted keys and revoked grants.

Assembly never fails on a bad row. A malformed permission, an undecodable key
digest or a credential with two owners is dropped, counted and logged; one bad
row must not take down an instance that is otherwise serving traffic.

### Decisions this crate makes, that the schema does not

- **`api_keys.key_hash` is the lowercase hex of the SHA-256 of the key text.**
  The column is a plain `String` and nothing in `gproxy-store` fixes an
  encoding, so it is fixed here: every writer and reader goes through
  `snapshot::encode_key_hash` / `decode_key_hash`. Which digests to compute
  from a presented key — the raw text, and again with an `sk-` prefix removed —
  is the authentication ladder's business, not the index's.
- **A credential with several owner columns resolves user > team > org.** The
  entity says management sets exactly one; a row that has more is malformed,
  and the narrowest owner is the one that leaks least.
- **A permission decision is the first applicable rule in `(priority DESC, id)`
  order.** A deny reached first wins, an allow reached first grants, and no
  applicable rule denies. Priority is how an exception is carved out of a broad
  rule in either direction; two rules that disagree at the *same* priority fall
  back to id order, so overlapping allow/deny rules should have distinct
  priorities.
- **A permission row that names both a user and an API key is their
  intersection**, never their union.

### Enforced twice, on purpose

The OAuth client allowlist is evaluated here *and* in SQL.
`gproxy_store::operations::oauth_policy` builds the same condition into the
statement that performs the authorization, so a policy change concurrent with
an authorization cannot slip between a check and an insert. `ClientAllowlist`
is the fast path: refusing early, listing clients, rendering a portal, with no
round trip. The SQL one is the authority, and the two must agree. One level is
missing here and cannot be added — the global allowlist lives on `settings`,
which is `ControlData`, not `IdentityData`.

## Authentication

Three kinds of caller, one `Caller`. Nothing above this layer re-derives who is
asking.

| Kind | Credential | Binding |
|---|---|---|
| `ApiKey` | a gateway key in `Authorization: Bearer`, `x-api-key` or `x-goog-api-key` | the key row's organization, team and subscription |
| `OAuthGrant` | an issued access token, resolved through `resolve_access_many` | the grant's internal key row, plus a `GrantContext` |
| `Session` | a console or portal cookie backed by `user_sessions` | none: a session acts as the person |

A key in the query string (`?key=`, the Gemini shape) is deliberately not read
from the header map. Query strings belong to the transport; a host that accepts
one extracts it and calls `authenticate_token`.

### The digest ladder, and why `sk-`/`at-` come off

A presented token is looked up under **two** digests, in this order:

1. `SHA-256(token)` — what `generate_api_key` writes, so a v4 key is stored
   under the digest of its whole `sk-…` text;
2. `SHA-256(payload)`, where the payload is the token with one leading `sk-`
   or `at-` removed — what v3 wrote.

`sk-` and `at-` are presentation prefixes, not part of the secret. Some
channels refuse a token that does not look like a personal access token, so the
same key is handed to one upstream as `at-…` and to another as `sk-…`. Digesting
the payload keeps all three spellings — `sk-X`, `at-X`, bare `X` — one identity,
so quota and usage follow the key rather than how it was typed. A token with no
prefix yields one digest, not two.

### Rules that hold across all three

- **An OAuth key never authenticates directly.** `api_keys.kind = oauth` marks a
  grant's internal key: it carries the grant's binding and usage identity, and
  presenting its text as a bearer key is refused rather than treated as a miss.
  It does not fall through to the access-token path either.
- **Every failure is 401, never 403.** A disabled key, an expired key, a revoked
  grant and a key that never existed are indistinguishable from outside;
  `Forbidden` would say "this credential is real, but not entitled", which is
  the fact a probe is looking for. `AppError::Unauthorized` carries a reason for
  the operator's log and renders as the bare word `unauthorized`. 403 belongs to
  admission, after identity is settled.
- **Expiry is re-checked against the request clock.** The key index excludes
  expired rows when it is assembled, but a key can expire while that snapshot is
  still the live one.
- **Tokens are stored only as hashes.** `api_keys.key_hash`,
  `user_sessions.token_hash` and `oauth_tokens.token_hash` hold a SHA-256 of the
  text. The plaintext is returned once, at creation, and the instance cannot
  produce it again. The two string columns share `encode_key_hash`, so one
  encoding is used everywhere here.
- **Grant liveness is not re-implemented.** `resolve_access_many` checks the
  token, the grant, the client, the user, the internal key, the subscription and
  the client allowlist in the same statement that reads the token, so a
  concurrent revocation cannot slip between a check and a use.

### Where CSRF applies

`verify_same_origin` is a standalone function, not a step in the ladder, because
it applies to **cookie-authenticated requests only**. A cookie is attached by the
browser to any request that reaches the origin, including one a foreign page
caused; a header-borne API key is not, and a foreign page cannot read it.
Running the check against API-key callers would break every non-browser client
for no gain. The host calls it after authentication, on a `Session` caller,
before the operation runs.

Safe methods (`GET`, `HEAD`, `OPTIONS`) always pass. Everything else needs an
`Origin` that matches the request's own `Host` or one of `cors_origins`. Unlike
v3, a missing `Origin` on an unsafe method is refused: browsers send one, so its
absence is not a browser, and treating it as trusted is an opt-out any attacker
can take.

### Passwords

argon2id, a fresh 16-byte salt per hash, stored as a full PHC string so raising
the parameters later does not invalidate every existing password. The policy is
v3's — a password must not be blank — plus a minimum length of 8 characters and
a maximum of 1024 bytes. No composition rule beyond that: mandatory character
classes push users towards `Password1!` and forbid long passphrases. A
`password_hash` that does not parse is a user who cannot log in, never a panic
and never an accidental success.

## Admission

`Admission::admit` takes a `Caller` and a request and produces the `Admitted`
the engine runs with. Six steps, in this order:

| # | Step | Produces | Can refuse with |
|---|---|---|---|
| 1 | permissions | the allowed provider set | `403 forbidden` |
| 2 | credential visibility | the allowed credential set | — |
| 3 | the budget chain | who this request is charged to | — |
| 4 | the scope | whose traffic it counts as, for affinity | — |
| 5 | the session identity | which conversation it continues | — |
| 6 | rate limits | the charges it is holding | `429 rate_limited` |

Steps 1–5 are pure functions of the snapshot and the request. **Step 6 is
last because it is the only one that consumes something.** A request rejected
for permissions must not move a shared counter, or an unauthorized client
could exhaust a legitimate caller's window by sending requests it was never
going to be allowed to make.

### What an instance administrator bypasses

`users.role = "admin"` **bypasses the permission filter** (every provider) and
**credential visibility** (every credential). That role already grants the
operations that write the `permissions` table, so a rule that refused an admin
a provider would be a rule they could delete; making the bypass explicit keeps
a mis-scoped deny from locking the operator out of their own instance.

It bypasses nothing else. Rate limits still apply, budgets still apply, and —
deliberately — **the OAuth baseline below still applies**. That restriction
protects the account holder from the client they authorized, and an admin's
account is exactly the one where a third-party token must not become an
administrative credential.

### Credential visibility follows the key, not the person

A credential is visible when it is unowned (`Shared`), or when its owner
matches the **calling key's binding**: `api_keys.user_id`, `api_keys.team_id`,
or the key's effective organization — its own `organization_id`, or the parent
of the team it is bound to.

It is the key's binding rather than the holder's memberships because a user
can belong to two organizations while a key belongs to one. If membership
decided it, every key of a multi-org user would reach every organization's
subscriptions, and no key could ever be narrower than its holder. The binding
is on the row: the client does not send it and cannot choose it, and it is the
same value the budget chain and the permission subject read, so what a request
may see, what it may spend and who pays cannot disagree.

Visibility narrows the engine's live credential set; it never adds to it. An
empty result is **not** an error here — "permitted nothing" and "nothing
exists to permit" are different failures, and the second is resolution's
`NoTarget`, a configuration problem rather than an entitlement one.

### The budget chain

`[api_key?, user, subscription?, team?, org?]`, skipping the parts that are
not set, with the bare kind strings `api_key` / `user` / `subscription` /
`team` / `org`. Core matches those verbatim against `quotas.owner_kind` and
knows no hierarchy between them: **every** enabled budget of **any** owner in
the chain applies, so the order is what a log reports, not a precedence.

`org` is the key's `organization_id` exactly as stored. Unlike credential
visibility, a team-bound key does not also charge the team's parent
organization: reaching a shared credential is what a team is for, but a budget
is a row an operator wrote against one named owner.

An OAuth grant has the same chain as the key behind it — a grant spends the
account it was issued against, not a budget of its own. A console/portal
session has no key, so its chain starts at `user`.

### The scope, and why a grant gets its own

`user:{user_id}`, except for an OAuth grant, which is `grant:{grant_id}`. Core
uses the scope for credential affinity and never parses it.

Every key of one user shares a scope: keys are the same person's credentials
by construction, and isolation between them is credential visibility's job.
A grant is a *different program* acting for that person, and mixing it with
their own traffic would hand the user a continuation belonging to a program
they are not running, let one client reference upstream resources another
created, and leave a revoked client's bindings attached to the user after the
grant that made them is gone.

### The OAuth operation baseline

An access token is a credential a user handed to somebody else's binary.
Unless its `client_id` is listed in `AppConfig.oauth.cli_client_ids` (empty by
default), it may only perform `ListModels`, `GetModel`, `CountTokens`,
`GenerateContent`, `StreamGenerateContent` and `CompactContent`; anything else
is `403`. Naming a client there is an operator accepting that it speaks for
the user across the whole API.

### Rate limits fail closed

Rows come from the snapshot, counters from the cache, because several
instances share one limit and a per-process counter would multiply every limit
by the number of instances. Windows are fixed and aligned to the epoch —
`now - now % (period_seconds × 1000)` — so every instance agrees on the
boundary from the clock alone, and a counter's key is
`rl/{row_id}/{window_start}`.

`metric = "concurrency"` takes a cache permit held for the life of the
request; every other metric increments a counter with the limit as its
ceiling, by exactly 1. A permit is **not** keyed by window — its key is
`rl/{row_id}/live` — because it measures requests in flight, which no window
boundary divides; the period is only how long the cache waits before
reclaiming a permit from a request that died without releasing. A limit that wants to cap *tokens* is a budget, not a
rate limit: the token count does not exist until the upstream has answered.

**A cache that cannot answer refuses the request** (`429`, with no retry
hint). `design/cache.md` is explicit that a failing cache must not fall back
to local state; the same reasoning applies here, because passing the request
turns a cache outage into "every limit on the instance is off" — the one
moment an attacker wants and an operator cannot see.

Charges are returned when they should be. A row that rejects gives back
everything earlier rows in the same request took. `Admitted::finish()` says the
request ran, which keeps a window counter but still returns a concurrency
permit — the permit measures requests in flight, not requests made. Dropping a
lease returns whatever is still outstanding; because the cache is async and
`Drop` cannot await, the release is spawned, and a host that can await should
call `Admitted::release()` instead. Every charge expires with its window, so
even a lost release self-heals at the boundary.

## The data plane

`App::call` and `App::connect` are the whole bridge to the engine: admit, then
drive the handle's builder with every value out of the `Admitted` — scope,
attribution, budget owners, the provider set, the credential set, the session —
and with nothing that was not decided there. The engine never sees a `Caller`,
so it cannot re-answer a question this layer already answered, and this layer
never guesses at routing, which is the engine's.

One value is read on both sides: the model name. It is parsed out of the JSON
body here (the handle's own helper for that is private, so the same two lines
exist in both places) and then passed to the builder explicitly, so the name
permissions and rate limits were decided against is the name resolution uses.
A dialect that carries the model in the path instead — the Gemini shape — has
already been parsed by the host, which sets `DataPlaneRequest::model`.

The gateway's session header is stripped before forwarding, here as well as
inside the handle. A client must not be able to choose another caller's
conversation by sending the header the gateway uses to name one.

### Keep the leases alive while you stream

`CallOutcome` hands back the `Admitted` on purpose:

| Charge | On `finish()` | On drop |
|---|---|---|
| window counter (`requests`, …) | stands — a request was made | nothing |
| concurrency permit | still outstanding | returned |

`call()` has already called `Admitted::finish()` by the time it returns, so
**the host must hold `outcome.admitted` for as long as it is writing the
response**. A concurrency permit measures requests *in flight*, and a streamed
answer is still in flight long after `call()` returned; dropping the outcome
early hands the slot to the next request while this one is still using it. A
host that can await should end the request with `Admitted::release()` rather
than relying on the drop path, which has to spawn because `Drop` cannot await.

A failure after admission is the opposite case: `call()` releases every charge
before it returns, because nothing ran.

`SdkError` is mapped to `AppError` with its status preserved. Engine, store and
cache failures are unwrapped into the variants this crate already has — so
matching on `AppError::Core` still catches a spent budget whichever layer
raised it — and everything else stays whole in `AppError::Sdk`, because the
handle has already decided what a 429 from every target, or a name that
resolved to nothing usable, is worth on the wire.

## Vendor services

The service views (`profile`, `usage`, plugins, remote control) have no
`OperationKey` and no model. Core says outright that it does not decide who is
an admin of what, so this crate answers exactly two questions and hands the
rest over:

1. **the target's credentials** are the caller's *visible* credentials of that
   provider — the same set a model call from the same key could spend. `Pool`
   therefore aggregates only what this caller already reaches, and
   `Credential(id)` can only name one of them (anything else is `404`);
2. **the role** is `Admin` for an instance administrator, or for an `Admin`
   member of the organization or team that owns **every** credential in that
   set. Anything else is `Member`, and a `Member` may only ask for the `Caller`
   view — `Pool` and `Credential` are `403`.

"Every credential" is not a formality: the target is the whole visible set, so
administering one organization must not render a pool that also contains
another's. An unowned credential has no administrator by construction — nobody
is an `Admin` member of nothing — so on a single-tenant instance where every
credential is shared, only an instance administrator reaches `Pool` and
`Credential`. Widening that is an operator decision (promote the user, or give
the credential an owner), not an inference made here.

A service takes **no rate-limit lease** — charging a window for a profile fetch
would spend an allowance the caller needs for traffic that costs money — but it
does pass the budget chain, because the `Caller` view renders the caller's own
windows from it. Reporting a quota is not spending one.

`ServiceRequest::user_id` is `attribution(caller).user_id`, from the same
function a model request uses. Core's view of the caller reads it to find the
caller's usage; two answers to "who is this" inside one request is exactly the
bug the single-`Caller` rule exists to prevent.

And no capture on either side. Core writes no upstream record for a service, so
a downstream one would have nothing to link to and would read as a request that
reached no upstream; there is also no `Admitted` to attribute it to. A vendor's
profile page is the one thing a request log is not for.

## Downstream capture

`capture_records` has two sides and they have two owners. Core's `StoreObserver`
writes the `side = Upstream` row for every physical send and the settled
`usage_records` row; only a host sees the inbound HTTP exchange, so the
`side = Downstream` row is this crate's. `design/core-observation.md` states
the rule — the host owns the downstream record, the edges are built once it
exists, and core never fabricates one — and `src/capture.rs` is that half.

| | upstream (core) | downstream (here) |
|---|---|---|
| gate | `enable_upstream_log` | `enable_downstream_log` |
| body gate | `enable_upstream_log_body` | `enable_downstream_log_body` |
| redaction | `disable_log_redaction` | the same switch, the same field list |
| id | its own, opaque | **the request id**, which is also the usage row's |
| body storage | `capture_events`, streamed | the inline column, buffered and capped |

All four switches are read off the `settings` row of the revision the request
pinned, at the same load that assembles `AppData` — not from `AppConfig`, and
not from a second copy of the settings.

`App::call` opens the capture after admission (an attribution column is an
admission decision, so a request refused before one has no record) and hands it
back inside `CallOutcome` with the response head already recorded. The host
feeds it the chunks it writes and ends it:

```rust
capture.record_response_chunk(&chunk);
capture.settle(store, CaptureOutcome::Complete, usage).await;
```

`settle` awaits the `UsageCompletion`, which is what makes core settle; a host
that wants the `UsageReport` for itself awaits it, calls `link_exchanges` and
then `finish`.

### Ask before you clone

With `enable_downstream_log` off, `DownstreamCapture::open` answers `None`:
nothing is allocated, no body is copied, no row is written. With
`enable_downstream_log_body` off the bodies are not copied either and
`*_body_state` says `NotCaptured`, which is how a reader tells "there was no
body" from "we chose not to keep it". What is kept is capped at 64 KiB per
direction — the same cut the sdk's log detail applies on the way out — and a
body over it is stored truncated with the state saying `Partial`.

Redaction masks the header names, query parameters and JSON fields on core's
own list (`authorization`, `cookie`, `api_key`, `access_token`, …), so the two
sides of one request hide the same things; core's helper is private, so this is
a second implementation of the same list rather than a call. A request body is
redacted **before** it is cut, so length is never a way past the policy. A body
that is not JSON has no key to match on and is stored as received — one more
reason the body switch is off by default.

### The edges

`capture_links` is where "this request reached those upstreams" lives, and a
retried request has one row per attempt. Two sources are merged, because
neither is complete on its own:

- `UsageReport::exchanges`, which names what core could **meter**. A `503` that
  was retried elsewhere carries no usage, so it is not in the report at all;
- the `capture_records` whose `initiator_request_id` is this request — the
  column the schema keeps so a retry stays traceable. It misses an upstream
  record a *continuation* reached, which carries another request's initiator
  and is only named by the report.

`sequence` is a dense rank over `(started_at_ms, attempt_ordinal)`, not the
upstream's own ordinal: a failover restarts the engine's attempt counter at the
next provider, so two attempts of one request can both be ordinal 1. Two
attempts that began in the same millisecond share a number, which is what the
column means by "parallel calls may share an ordinal; break ties by
upstream_id".

Nothing is written until the exchange ends: the record and all of its edges go
in one batch. With `enable_upstream_log` off no edge is written at all — there
is no upstream row to point at, and the foreign key would take the record down
with it.

### A capture failure is never a request failure

The same rule core follows. `finish` logs at `error` and returns; the request
has already been answered by then and an `Err` must not become a response.
`App::call` discards it on its own error path. If the batch fails because an
edge pointed at a record that was never written, the record alone is retried:
losing the request's log line over a missing edge is the worse of the two.

`provider_id`, `credential_id`, `pool_id` and `metrics` stay unset on a
downstream row. A retried request reached two providers with two credentials
and a single column would have to pick one; the edges answer that without
picking, and billed usage is the `usage_records` row.

## Publication links

Core owns published bytes and the id they are stored under, but it has no
public HTTP surface, so it cannot say where they can be fetched from.
`AppPublicationUrl` answers `{public_base_url}/publications/{id}`, and
`App::read_publication` / `App::delete_publication` serve that route.

**With no `public_base_url` configured it answers `None`**, which makes the
publish fail with `Unsupported` before any body is written, and the caller is
told to ask for the bytes inline (`b64_json`) instead. The alternative would be
to build the link from the request's `Host` — chosen by the client — or from an
`x-forwarded-proto` chosen by whatever is in front. A link built from a guess
is worse than no link: the upstream answer is accepted, the bytes are stored,
the URL is handed out, and it 404s somewhere else.

A read that finds nothing, a released body and an expired one are one answer —
`404` — because distinguishing them tells a prober which ids existed. The scope
is not checked: the id is a random secret and the link is the capability,
exactly as core's own read has it.
## Identity operations

`Operations::new(&gproxy, &data, &config)` hands out one accessor per family.
Every family is `list / get / create / update / delete` over one table, from a
single generic shape, plus whatever else that family genuinely needs.

| Family | Table | Beyond the five |
|---|---|---|
| `users()` | `users` | `set_password`, `clear_password`, `set_allowlist`, `batch` |
| `api_keys()` | `api_keys` | `create` returns the plaintext once, `reveal`, `rotate` |
| `organizations()` | `organizations` | `batch`; delete performs the key cascade |
| `teams()` | `teams` | `batch`; delete performs the key cascade |
| `members()` | `organization_members` | `add`, `set_role`, `remove` over the composite key |
| `team_members()` | `team_members` | `add`, `set_role`, `remove` over the composite key |
| `permissions()` | `permissions` | `batch` — a rule set is edited as a whole |
| `rate_limits()` | `rate_limits` | `batch` |
| `subscriptions()` | `subscriptions` | `batch` |
| `pools()` | `subscription_pools` | `batch` |
| `pool_members()` | `subscription_pool_members` | `batch` |
| `plans()` | `subscription_plans` | `batch` |
| `plan_limits()` | `subscription_plan_limits` | `batch` |
| `oauth_clients()` | `oauth_clients` | `retire` instead of delete |
| `sessions()` | `user_sessions` | `list`, `revoke`, `revoke_all`, `purge_expired` |
| `audit()` | `audit_events` | `record`, `try_record`, `query` |

`Operations` performs **no authorization**. Who may call which family is the
host middleware's decision, taken from the `Caller` before the operation runs.

### One batch, one revision, then a notification

A configuration write is one `Store::commit_revision`: the caller's statements
and the `settings.config_revision` bump land together or not at all. A write
that landed without a bump would be invisible to peers; a bump without a write
would make them reload for nothing. A create and an update read their row back
from **inside** that same transaction, so the DTO returned is the row as the
revision left it rather than a re-read a peer could already have changed.

Afterwards the writer publishes `Invalidation::ConfigurationChanged { revision,
scopes }` on `gproxy-core`'s invalidation topic, with this crate's own scope
names: `identity`, `permissions`, `keys`, `rate_limits`, `subscriptions`,
`oauth_clients`. Publication is best effort — a cache that refuses it costs the
deployment one poll interval, not correctness.

**How this differs from the sdk's `manage::Writer`, and why.** The sdk reloads
and only then notifies, because a peer must never find the writer still serving
the revision it just announced. This layer does not reload at all: identity
rows are not in `CoreData`, and what they feed — `AppData` — is owned by the
host, which holds the `AppSnapshot` and rebuilds it through `App::refresh()`. A
writer that reloaded on its own would publish a second, competing snapshot and
lose the monotonicity `AppSnapshot` exists to guarantee. So the contract is:
**the operation commits and notifies; the host refreshes.**

Two families are not configuration and deliberately do neither. `user_sessions`
is not in `IdentityData`, so authentication reads it on every request and a
revocation takes effect without anything reloading; `audit_events` is history
that nothing reads to serve a request. Bumping the revision for either would
make every login and every audited operation invalidate the whole fleet's
snapshot for a change no snapshot contains.

### What is never returned

`users.password_hash`, `api_keys.key_hash`, the bytes of `api_keys.secret` and
`user_sessions.token_hash` have no DTO field and no accessor. A user reports
`hasPassword`, a key reports its display `prefix` and `hasSecret`, a session
reports only when it was opened and when it ends.

A key's plaintext exists in a response exactly twice: `create` and `rotate`
return it once. `reveal` can produce it later only for a key created with
`retainSecret`, which seals a copy under core's `SecretCodec` bound to the
key's own id — so a sealed blob copied onto another row does not open.
Retention is off by default: a key the instance cannot reproduce is a key a
database leak does not hand over.

### The explicit key cascade

Deleting an organization deletes, in the same batch and before the parent row:
every key bound to one of its teams, every key bound to the organization
itself, and the teams. Deleting a team deletes the keys bound to it. This is
the rule the section below exists for, and the reason it is written out rather
than left to the schema.

Deleting rather than unbinding is deliberate. Unbinding would silently widen a
key from "this team's credentials" to "everything its user can reach" — a
privilege *increase* performed by a deletion — and the entity's own contract
says a key whose owner chain and visibility boundary are gone must stop serving
requests. Because the explicit statements run first, a database that *does*
have the foreign key finds nothing left to cascade, so a fresh database and an
upgraded one end in exactly the same state.

### Validation the schema cannot express

- **A key's binding is checked three ways**: the organization and team exist,
  the key's user is a member of them, and when both are set the team's parent
  *is* that organization. A patch is re-validated against the binding the row
  will end up with, not the half it mentioned. On the upgrade path these
  columns carry no foreign key at all, so for two of the three this is the only
  check there is.
- **A key may only select its own user's subscription.**
- **A permission rule and a rate limit take exactly one subject.** Neither is a
  row `PermissionSet::build` drops on the floor; both is an intersection the
  snapshot still honours — a v3 database may hold one — but which nobody writing
  a rule means, so the write is refused rather than silently misread later.
- **`action` is `allow` or `deny`**, `role` is `admin` or `user`, a membership
  role is `member` or `admin`, a plan period is one of `total / fixed / day /
  week / month`, and a `fixed` window needs a positive duration.
- **`periodSeconds` is positive** and **`limitValue` is not negative**; zero is
  a legitimate ceiling that switches a subject off without deleting its row.
- **`startsAtMs <= expiresAtMs`** whenever both are known.
- **The last enabled administrator cannot be deleted, disabled or demoted.**
  The instance role is not a membership and cannot be granted by anybody who is
  not already an admin, so losing it is unrecoverable without going to the
  database. The count is read from the database rather than from the snapshot:
  a snapshot that is one revision behind would still list an admin deleted a
  moment ago and would wave through the deletion of the real last one.
- **A password change ends the user's sessions**, in the same transaction — a
  session that outlives the credential that opened it is exactly the window a
  reset is meant to close.
- **An OAuth client is retired, never deleted**, and retirement revokes its
  grants, their internal keys and every issued token through the store's
  atomic `retire_many`. Re-registering the same `client_id` is refused, so a
  fresh row can never inherit the previous registration's grants.

### Audit, and what `detail` never carries

`audit().record(AuditEntry { … })` appends one row **outside** the revision
batch. That is not laziness: a trail insert that failed would otherwise roll
back the operation it was only describing, and a *rejected* operation — the row
an investigation actually wants — has no transaction to join at all. A failure
to write the trail is logged, never propagated.

`AuditEntry::redacted(value)` is the only supported way to put caller-supplied
JSON into an entry, because redaction happens before the value is stored, not
before it is displayed: a secret that reaches the column has already leaked
into every backup and replica. It walks objects and arrays to any depth and
replaces the value of any field whose name — ignoring case, underscores and
dashes — is one of:

```text
accessToken  apiKey     authorization  clientSecret  code        codeVerifier
cookie       credentials  currentPassword  idToken    key         keyHash
masterKey    newPassword  oldPassword   password     passwordHash  privateKey
refreshToken secret      sessionToken   setCookie    token       tokenHash
verifier     xApiKey
```

with the literal string `[redacted]` — replaced rather than removed, so a
reader can tell "this operation carried a password" from "this one did not".
Matching is on the whole normalized name, not a substring, so a
`passwordPolicy` or a `keyboardLayout` survives. `AuditEntry::failed(&error)`
records the error's stable **code**, never its message, because a message can
quote a value the caller sent.

`query(AuditQuery)` pages newest first — `created_at_ms DESC`, then the primary
key, so two rows written in the same millisecond cannot repeat or skip across a
page boundary.

## OAuth issuer

`Operations::issuer()` is gproxy acting as an **OAuth authorization server for
downstream clients** — an editor extension, a coding CLI, a script on the
user's machine. It is not how gproxy logs in to an upstream; that is the sdk's
`login()`, which speaks somebody else's OAuth as a client. Nothing in this
module ever talks to a provider.

```text
  Claude Code ──authorize/token──▶ gproxy  (operations::issuer: the server)
                                     │
                                     └──login()──▶ OpenAI   (gproxy-sdk: the client)
```

| Operation | Endpoint a host maps it to | RFC |
|---|---|---|
| `authorize_details(caller, query)` | `GET /oauth/authorize` — validate, then draw a consent screen | 6749 §4.1.1 |
| `authorize(caller, query, decision)` | `POST /oauth/authorize` — a code, or an error redirect | 6749 §4.1.2 |
| `token(request)` | `POST /oauth/token` — `authorization_code`, `refresh_token`, `urn:ietf:params:oauth:grant-type:device_code` | 6749 §4.1.3/§6, 8628 §3.4 |
| `device_code(request, origin)` | `POST /oauth/device/code` | 8628 §3.1 |
| `device_details(user_code)` | the approval page | 8628 §3.3 |
| `device_decision(caller, user_code, decision)` | the approval page's button | 8628 §3.3 |
| `device_cancel(client_id, device_code)` | a device retiring the code it printed | — |
| `revoke(request)` | `POST /oauth/revoke` | 7009 §2 |
| `metadata(origin)` | `GET /.well-known/oauth-authorization-server` | 8414 §2 |

This module is not HTTP: there is no routing, no form parsing and no
`Location` header here. `error_body(&AppError)` is what makes both hosts render
a failure identically — an `AppError::OAuth` keeps the RFC code it carries,
anything the instance is responsible for becomes `server_error`, and nothing
else quotes an internal message to a client.

### What a grant is

One authorization binds a **user** and an **internal API key** of
`kind = OAuth`, created in the same transaction. Everything downstream then
works on keys: the budget chain, the credential-visibility boundary and the
permission subject all come from that key row exactly as they would for a key
the user minted by hand. The key is never presentable — authentication refuses
a `kind = OAuth` key offered as a bearer key — so the only way to use it is to
present an access token, which resolves the whole chain in one statement.

**What an OAuth caller may then do is not decided here.** The operation
baseline — list models, get a model, count tokens, generate, stream, compact,
and nothing else unless the client is named in `oauth.cliClientIds` — lives in
[admission](#the-oauth-operation-baseline), because it is a property of every
request the token makes and not of the moment it was minted. Widening it must
not require re-issuing tokens, and narrowing it must take effect on the tokens
that already exist.

### Rules this issuer does not bend

**PKCE S256, always.** `plain` is refused and so is an absent challenge. Every
registered client is public, with no secret to prove with, so the verifier is
the only thing standing between a leaked code and a token. The *spelling* of
`code_challenge_method` is read leniently (trimmed, case-insensitive) because
there is only one transformation to name.

**Redirect URIs match exactly.** Not a prefix, not a wildcard, not
"same origin". Every looser rule has the same failure: a client registered for
`https://app.example/cb` would also accept `https://app.example/cb.evil.example`,
`https://app.example/cb/../../elsewhere` or `https://app.example/cb?next=//evil`,
and each of those is a URL an attacker controls that receives the authorization
code. The registry refuses to store a `*` for the same reason.

**Codes, refresh tokens and device codes are single-use.** The consumption and
the replacement are one atomic batch in the store's `exchange_tokens_many`, so
two redemptions of the same credential cannot both succeed however they are
interleaved.

**Tokens exist in exactly one response.** 32 bytes from `getrandom`, base64url
without padding; the row keeps the SHA-256 and nothing else —
`oauth_tokens.token_hash`, `oauth_codes.code_hash` and
`oauth_devices.device_code_hash` are `Binary(32)`. The hash function is the
same one `api_keys.key_hash` and `user_sessions.token_hash` use; only the
encoding differs, because those two columns are text. A lost token is
replaced, never recovered.

### Rotation reuse revokes the whole family

A code and a refresh token are single-use. When one is presented twice there
are two possibilities and no way to tell them apart from here: the client lost
a response and retried, or somebody else has the credential. RFC 6749 §4.1.2,
RFC 6819 §5.2.2.3 and the OAuth 2.0 Security BCP §4.14 resolve that the same
way — assume the leak.

So a replayed refresh token (or authorization code) answers `invalid_grant`
**and** revokes the grant, its internal API key and every access and refresh
token it ever issued, in one batch. The access token the legitimate client is
holding right now stops working too; it re-runs its login. The thief's does
too, and it cannot.

Detection is the store's, not a read-then-check: the exchange consumes the row
under `consumed_at_ms IS NULL`, so a spent credential comes back as
`CasOutcome::Conflict` and a re-read of the row says whether it was consumed
(a replay) or merely expired or revoked (a plain `invalid_grant`).

**The device flow is the exception.** A polling client re-sends the same device
code by design, and a lost response is indistinguishable from a delivered one.
A consumed device authorization is `invalid_grant` and nothing more — the
device code is spent either way, and taking the tokens with it would lock a
client out of an account it had just legitimately connected.

### The device flow reuses the code flow

An approval mints an ordinary authorization code, and the store's `issue_many`
reserves and approves the pending device row in the **same** batch. Two columns
make that work with no second shape:

- the code's `redirect_uri` is `urn:ietf:params:oauth:grant-type:device_code`,
  a URN no registration can hold (registration requires `://`), so a
  device-issued code can never be redeemed through `authorization_code`;
- the code's `code_challenge` is the base64url of `device_code_hash`, which
  *is* the S256 challenge of the device code, since both are the base64url of
  the same SHA-256. The device code is therefore the PKCE verifier, for free.

User codes are eight symbols from an alphabet with `I`, `O`, `0` and `1`
removed, shown grouped as `ABCD-EFGH` and normalized on lookup, so any spelling
with the right characters resolves. `slow_down` is never emitted: answering it
means a write on every poll of every device, and rate-limiting the endpoint is
the host's job and the same defence.

### `issuer` comes from the mount, never from a header

The same instance serves the issuer at up to three mounts — `https://host`,
`https://host/{namespace}/v1` and `https://host/{provider}/v1` — and RFC 8414
§2 requires the `issuer` identifier to be exactly the one a client fetched the
document from. Only the host knows which mount a request arrived on, so
`IssuerOrigin` is **passed in** and never computed here.

The scheme is the other half of that. `x-forwarded-proto` is a header any
client can send, so a host must believe it **only from a peer listed in
`trustedProxies`**, and fall back to `publicBaseUrl` or the socket's own scheme
otherwise. An issuer identifier taken from an attacker-supplied `Host` or
`x-forwarded-proto` is a discovery document pointing at somebody else's
endpoints.

`IssuerOrigin` keeps the origin and the mount apart because they are used
differently: the protocol endpoints hang off the mount, so a client discovers
the one it is talking to, while the device verification page hangs off the
origin — the portal is one application however many mounts the data plane
answers at.

### None of this moves the revision

Issuing, refreshing and revoking do **not** bump `settings.config_revision`,
the same decision `user_sessions` and `audit_events` made. A grant is resolved
by a database read on every request, so a peer's stale `AppData` cannot admit a
revoked token or refuse a live one. Bumping per exchange would instead make
every refresh invalidate every peer's snapshot — on a fleet with hour-long
access tokens, a reload storm in exchange for nothing. The one OAuth operation
that *is* configuration is retiring a client, which changes the allowlist
`AppData` holds, and it commits a revision in `oauth_clients()`.

Every issuance, denial, refresh, replay and revocation is written to the audit
trail with `try_record`, and that is load-bearing rather than lazy: a trail
write that turned a successful exchange into a 500 would have the client retry
with a code it has already spent, and the replay rule would then revoke the
grant it had just been given.

## The portal

The families above are an operator's tools: they take an id and act on
whatever row it names. The portal is the other half of the product — the
person who holds a key, signing in to see their own keys, their own spend, the
models they may call and the programs they have authorized.

```rust,ignore
let portal = Operations::new(&gproxy, &data, &config).portal(&caller);
portal.context().await?;              // the one call a portal loads first
portal.models()?;                     // every name, each marked `permitted`
portal.keys().create(write).await?;   // mints for the caller, nobody else
portal.usage(query).await?;           // the caller's own aggregate
portal.quota().await?;                // the caller's own budget chain
portal.recent_requests(20).await?;    // reduced, and behind a switch
portal.oauth_sessions().list().await?;
portal.password().change(change).await?;
```

| Operation | Answers |
|---|---|
| `context()` | user, organizations, teams, subscription, feature flags |
| `models()` | every exposed name and `channel/model` form, each with `permitted` |
| `keys()` | `list`, `create`, `rotate`, `reveal`, `delete` over the caller's own |
| `usage(query)` | summary, optional grouped cut, optional trend |
| `quota()` | the current window of every budget in the caller's chain |
| `recent_requests(limit)` | the caller's own recent requests, reduced |
| `oauth_sessions()` | `list`, `revoke` over the caller's own grants |
| `password().change(..)` | prove the old one, set the new one, sign out everywhere |
| `sessions()` | where the caller is signed in |
| `logout(token)` | end the session this request arrived on |

`Operations::portal_login(name, password)` and `portal_logout(token)` sit on
`Operations` rather than on `Portal`, for the one reason that matters: a
sign-in runs *before* there is a caller, and a type whose whole contract is
"every method is scoped to this caller" cannot also hold the method that
precedes one. **Rate limiting of sign-in attempts is the host's.** This crate
never parses a forwarding header and so has no client address; a limit keyed on
anything else would either lock one username out globally or limit nothing.

### Scoping is by construction, not by checking

**No portal method takes a user id.** There is no parameter through which a
caller could name somebody else, which is the whole design: a check can be
forgotten, a missing parameter cannot.

The one request field that looks like an exception is `PortalUsageQuery.userId`,
which exists so a console can post the same filter object to both surfaces. It
is **overwritten** with the caller's own id before the query reaches the engine
— written into the filter, not compared against it. The tests send another
user's id there and assert it was ignored.

Where an id genuinely is unavoidable — a key, a grant — the row is read and its
owner compared to the caller before anything happens.

### `NotFound`, never `Forbidden`

A portal operation on a row that belongs to somebody else answers **404**. A
403 would confirm that the id exists, which is exactly the fact an enumeration
is probing for; from outside, another user's key and an id that was never
minted must be one answer. The same 404 covers a grant's internal `oauth` key,
which the portal does not manage at all.

`Forbidden` does appear in this surface, for the one thing that names no row:
an **OAuth-grant caller** may not mint, rotate, reveal or delete keys, and may
not change the account password. A token the user handed to somebody else's
program must not be able to mint a fresh long-lived credential or lock the
owner out of their own account. That is a policy about the kind of credential
in hand, the caller can act on being told, and `context().features` reports it
up front so a portal can hide the button rather than render one that refuses.

### It reuses the admin families

A portal key create is `api_keys().create` with the caller's own user id filled
in, so the three binding rules — the organization and team exist, the caller is
a member of them, a team's parent is the named organization — are validated in
one place. A portal user who names an organization they do not belong to is
refused by that check, not by a second copy of it here. A password change is
`users().set_password` once the old password has been proved, which is also why
it ends **every** session including the one that asked: a session caller
carries no session id by design, and signing out everywhere is what a password
change is for anyway.

### What the portal deliberately sees less of

`recent_requests` returns no bodies, no headers, no URL, no client address, no
credential id and no provider id — only `requestId`, the caller's own
`apiKeyId`, the model, the operation, the provider's **display name**, the
status, the capture state and the timings.

A captured body can hold the caller's own prompt, which is theirs; it can
equally hold a system prompt, a tool definition or an upstream error that
belongs to the operator, and no read-side filter can tell the two apart. A
credential id and a provider id are infrastructure the account holder has no
use for and an attacker does. The provider survives as a name because "which
upstream served this" is a fair question. The operator's log view keeps all of
it — that is what it is for.

The list is gated by **`settings.portal_recent_requests_enabled`**, and this is
that column's only consumer. Off answers with an empty list rather than an
error: the switch is an operator's decision about what the portal shows, not a
statement about this caller, and a 403 would invite them to go looking for a
permission they are not missing. `context().features.canSeeLogs` carries the
same value so the tab can be hidden instead.

### The model list omits nothing

`models()` lists every exposed name and every `channel/model` form, and marks
each with `permitted`. A name the caller's rules do not reach stays in the list
with `permitted: false`. v3 dropped such rows; this does not, because a list
that silently omits makes "this model 404s" and "you are not allowed this
model" the same observation — and there is nothing to protect: an exposed name
and a `channel/model` form are instance configuration, the same strings the
operator publishes. What is withheld is the provider ids behind them; the DTO
reports a count and a channel, which say how redundant a name is without naming
the machinery.

`providerName/model` also resolves and is deliberately **not** listed: its left
half is a renameable row, so printing it would hand users a name that stops
working when somebody edits a provider.

`permitted` is computed by `admission::permission::allowed_providers` — the
same function the request funnel calls, against the same snapshot — evaluated
for `GenerateContent`, because the portal's question is "what can I send a
prompt to".

## A cascade that only exists on new databases

`api_keys.organization_id` and `api_keys.team_id` declare
`on_delete = "Cascade"`, but the SQLite incremental upgrade path adds those
columns with `ALTER TABLE ADD COLUMN`, which cannot carry a foreign key. Only a
freshly created database has the constraint; an upgraded one does not, and the
other backends are unverified.

**So this crate deletes the affected API keys itself when it deletes an
organization or a team**, in the same batch and before the parent row — see
[The explicit key cascade](#the-explicit-key-cascade). A key whose budget
chain, permission subject and credential-visibility boundary no longer exist
must not keep serving requests, and on an upgraded instance nothing below this
layer will stop it.
