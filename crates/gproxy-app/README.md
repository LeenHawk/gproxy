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
  → sdk call()  the engine executes with exactly those, and nothing re-derives them
```

Each step narrows, and nothing below re-opens what was narrowed above: the
engine is handed a provider set and a credential set, not a caller to
interpret. The sdk bridge itself (`call.rs`, `service.rs`, `publication.rs`)
lands next; today this crate stops after the `Admitted`.

## What exists now

| Module | Holds |
|---|---|
| `config` | `AppConfig`: plain serde, shared by both hosts, no filesystem or environment access |
| `snapshot` | `AppData`, the compiled identity of one configuration revision, and `AppSnapshot`, its monotonic publication |
| `auth` | the three ways a request names a caller, and the `Caller` all three produce |
| `admission` | `Caller` → `Admitted`: providers, credentials, budget owners, scope, session, rate-limit charges |
| `error` | `AppError`, with `status_code()` and a stable `code()` for the API envelope |

Everything else (`call`, `service`, `capture`, `publication`, `operations`,
`audit`, `dto`) is declared with the contract it will hold, so a later phase
fills it in place rather than reshaping the crate.

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
boundary from the clock alone, and the key is `rl/{row_id}/{window_start}`.

`metric = "concurrency"` takes a cache permit held for the life of the
request; every other metric increments a counter with the limit as its
ceiling, by exactly 1. A limit that wants to cap *tokens* is a budget, not a
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

## A cascade that only exists on new databases

`api_keys.organization_id` and `api_keys.team_id` declare
`on_delete = "Cascade"`, but the SQLite incremental upgrade path adds those
columns with `ALTER TABLE ADD COLUMN`, which cannot carry a foreign key. Only a
freshly created database has the constraint; an upgraded one does not, and the
other backends are unverified.

**So this crate must unbind or delete the affected API keys itself when it
deletes an organization or a team.** A key whose budget chain, permission
subject and credential-visibility boundary no longer exist must not keep
serving requests, and on an upgraded instance nothing below this layer will
stop it.
