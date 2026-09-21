# gproxy-host-axum

English | [简体中文](README.zh-CN.md)

The native HTTP host for GPROXY v4: an [axum](https://docs.rs/axum) router over
[`gproxy-app`](../gproxy-app). It is a **transport binding** — it turns bytes
into the typed calls `gproxy-app` exposes and turns their answers back into
bytes, and it contains no product behaviour of its own.

Native only. It binds axum, hyper and tokio, none of which run on
`wasm32-unknown-unknown`; the edge deployment is a separate host over the same
operations.

```rust,ignore
let app = Arc::new(App::new(gproxy, config));
app.reload_all().await?;
let router = gproxy_host_axum::router(HostState::new(app));
axum::serve(
    listener,
    // Required: without it the peer address is unknown, and an unknown peer is
    // never a trusted proxy.
    router.into_make_service_with_connect_info::<SocketAddr>(),
).await?;
```

## The route table

| Method | Path | What it is |
|---|---|---|
| `GET` | `/healthz` | liveness and the published config revision; unauthenticated |
| `GET` | `/publications/{id}` | a published body; the id **is** the credential, so no key is asked for |
| — | `/admin/api/…` | the operator surface: one explicit `MethodRouter` per operation |
| — | `/portal/api/…` | the end user's surface, plus `login` and `logout` |
| — | everything else | the ingress fallback, in the order below |

### `/admin/api`

Every identity family has the same five routes, generated from one macro so a
family cannot accidentally have four of them or a different method on one:

```
GET    /admin/api/{family}           list      (page + filters in the query)
POST   /admin/api/{family}           create
GET    /admin/api/{family}/{id}      get
PATCH  /admin/api/{family}/{id}      update
DELETE /admin/api/{family}/{id}      delete    → 204
```

`{family}` is one of `users`, `api-keys`, `organizations`, `teams`,
`permissions`, `rate-limits`, `subscriptions`, `pools`, `pool-members`,
`plans`, `plan-limits`. `oauth-clients` has the first four and
`POST …/{id}/retire` instead of a delete: the grants a client issued still name
it. Beyond the five:

```
GET    /admin/api/session                      who this request is
DELETE /admin/api/session                      end it, clear the cookie
POST   /admin/api/users/{id}/password          set
DELETE /admin/api/users/{id}/password          clear
PUT    /admin/api/users/{id}/allowlist         OAuth client allowlist
GET    /admin/api/users/{id}/sessions          where they are signed in
DELETE /admin/api/users/{id}/sessions          sign out everywhere
POST   /admin/api/api-keys/{id}/rotate
GET    /admin/api/api-keys/{id}/secret         reveal a retained secret
GET    /admin/api/organizations/{id}/members   (+ POST, and GET/PUT/DELETE …/{userId})
GET    /admin/api/teams/{id}/members           (+ the same)
GET    /admin/api/sessions                     every session
DELETE /admin/api/sessions/{id}
GET    /admin/api/audit                        the trail
```

Middleware, in order: authenticate → **require the instance administrator** →
same-origin for an unsafe cookie request → the operation → an audit row for
every method that is not a read. It is a `route_layer`, so an unknown
`/admin/api/*` path is a 404 that never touches the database.

The audit action is derived from the matched route (`admin.api_keys.rotate`,
`admin.users.update`), so a new route cannot forget to name itself.

### `/portal/api`

```
POST   /portal/api/login             name + password → session cookie and token
POST   /portal/api/logout            ends the session and clears the cookie
GET    /portal/api/context           the caller, their scopes, their plan
GET    /portal/api/models            what they may call
GET    /portal/api/usage             their own spend
GET    /portal/api/quota             their own windows
GET    /portal/api/requests          their recent requests, if enabled
GET    /portal/api/sessions          where they are signed in
GET    /portal/api/keys              (+ POST)
DELETE /portal/api/keys/{id}         (+ POST …/rotate, GET …/secret)
GET    /portal/api/oauth-sessions    (+ DELETE …/{id})
POST   /portal/api/password          change, proving the current one
```

The guard here requires an authenticated caller and **nothing more**. There is
no role check because there is nothing to check against: every operation on
`Portal` is scoped to the caller it was built from and none of them takes a
user id. `login` and `logout` sit outside the guard — the first runs before
there is a caller, and the second has to work for a session that has already
expired, or a browser is left holding a cookie it can never discard.

### Ingress, in this order

1. **CORS and the client address.** A preflight is answered here and never
   forwarded. The client address is resolved under the trusted-proxy rule.
2. **The mount** — see the grammar below.
3. **The OAuth issuer**, at `{mount}/v1/oauth/…`, in the RFC's own error
   envelope.
4. **A channel's vendor service route** (Codex's `/backend-api/…`, Claude
   Code's `/api/…`). Only on a provider or namespace mount: matching a route
   needs a channel, and the aggregated mount names none. The view is chosen
   with `x-gproxy-view: caller | pool | credential:{id}`. A route the channel
   declares as a socket is upgraded instead of called.
5. **The data plane** — `/v1/messages`, `/v1/responses`,
   `/v1/chat/completions`, `/v1beta/models/{model}:generateContent`, the files,
   images, audio, video and realtime surfaces. The body is decoded, the
   operation is matched, `App::call` runs — or, for a handshake surface,
   `App::connect` and then the upgrade.
6. **The console**, with an SPA fallback, behind `console.enabled`.

Anything that reaches the end is a 404.

### WebSocket

| Route | Operation / dialect | What it is |
|---|---|---|
| `GET {mount}/v1/realtime` | `ConnectRealtime` / OpenAI | the realtime session, optionally continuing a call with `?call_id=` |
| `GET {mount}/v1/live` | `ConnectRealtime` / OpenAI | the same, at the WebRTC spelling |
| `GET {mount}/v1/live/{call_id}` | `ConnectRealtime` / OpenAI | the continuation with the call in the path |
| `GET {mount}/v1/responses/ws` | `GenerateContent` / OpenAI Responses-over-WS | the Responses websocket envelope |
| `GET {mount}/ws/v1beta/BidiGenerateContent` | `ConnectRealtime` / Gemini | Gemini Live |
| `GET {mount}/backend-api/…` | a channel `ServiceRoute` with `ServiceTransport::WebSocket` | Codex's remote-control server |

The three OpenAI realtime paths are exactly the routes
`Core::connect_realtime_path` dispatches, and a test asserts the two tables
agree. `POST /v1/realtime/calls` is **not** among them: the SDP offer is an
HTTP multipart request, and the handshake that continues it carries no body at
all (`WireRequest<()>` all the way down to `Upstream::connect`). What survives
the upgrade is the call id — in the path or in `?call_id=` — which core looks
up to pin the socket to the credential that answered the offer. The query is
forwarded intact for exactly that reason.

**Nothing is upgraded before it is allowed.** Authentication, then the
handshake shape, then admission, then the upstream handshake; the `101` is
written last. Every refusal is therefore an HTTP answer a client can read — a
socket that is accepted and immediately closed carries no status, no body and
no code.

**A refused upstream handshake is relayed verbatim.** The upstream's status,
headers and body reach the client as they stand; a vendor's
`429 {"error":{"code":"insufficient_quota"}}` is worth more than any 502 this
gateway could invent. That body is the one response this host buffers on
purpose: a client in the middle of a handshake reads what is left of the
response out of its handshake buffer, and a chunked relay would arrive with
its chunk framing still in it.

**A socket holds its lease for as long as it is open.** The pump owns the same
`Trailer` a streamed response body does — the `Admitted`, the downstream
capture and core's `UsageCompletion` — and releases it when the socket closes,
not when the `101` was written. A session that runs for an hour holds its
concurrency permit for that hour.

The pump forwards text and binary in both directions; ping and pong stay on the
side they were sent on, because a keepalive measures the link it travelled. A
close on one side is forwarded to the other and the closing handshake is then
read to its end (with a five-second grace period), which is what lets core
settle the exchange as complete rather than as an interrupted one. A frame over
`max_ws_frame_bytes` closes both sides with `1009 Message Too Big`.

**What is captured.** One `capture_records` row of kind `ws_connection` per
socket, with `101` as its status, plus one `capture_events` row per message —
text, binary, ping, pong and close, ordered across both directions, with a
`direction` each. Frames are gated on `enable_downstream_log_body` like any
other body and are redacted by the same rules; with it off the connection is
still recorded and nothing the caller said is copied. Recording stops after 64
KiB of payload and the row's body state says `partial`.

`turn_id` is always unset and no `ws_turn` record is written. The schema calls
it "the WS business turn, when identifiable", and a turn is a dialect's notion
— OpenAI's `response.created`/`response.done`, Gemini Live's own — while this
host forwards realtime frames as opaque passthrough (core refuses to convert
any websocket dialect but `OpenAiResponsesWebSocket`). Splitting turns here
would mean inventing a boundary the wire did not draw.

A vendor service socket holds nothing and records nothing, which is
`gproxy-app`'s rule rather than this crate's: services run outside the
observation funnel. Which credential it may speak for is the channel's
decision — Codex refuses a synthesized view outright, so `x-gproxy-view:
credential:{id}` from an administrator of that credential is the only way
through.

#### The OAuth endpoints

Relative to a mount prefix (`""`, `/acme`, `/openai-prod`):

```
GET  {prefix}/v1/oauth/authorize    consent handoff (302 to the portal for a browser)
POST {prefix}/v1/oauth/authorize    the person's decision
POST {prefix}/v1/oauth/token        code, refresh and device grants
POST {prefix}/v1/oauth/device/code
POST {prefix}/v1/oauth/revoke
GET  {prefix}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{prefix}/v1   (RFC 8414 §3.1)
```

The issuer identifier is `{origin}{prefix}/v1`. P7's module note sketched the
aggregated mount as `https://host`; that is corrected here to
`https://host/v1`, so one rule produces all three mounts and
`Issuer::metadata`'s `{issuer}/oauth/…` endpoints land on the paths above at
every one of them.

## What this crate refuses to decide

Nothing here re-answers a question that has an answer below it:

| Question | Who answers |
|---|---|
| who is calling | `gproxy_app::Authenticator` |
| what they may reach | `gproxy_app::Admission` |
| which provider serves a model | the sdk's resolver |
| what a failure is worth on the wire | `AppError::status_code` / `code` |
| what an operation does | `gproxy_app::Operations` / `Portal` / `Issuer` |

What it does decide: routing, header and body decoding, CORS, the client
address, the mount grammar, the two error envelopes, streaming, the websocket
upgrade and its duplex pump, and the lifetime of a rate-limit lease.

## The mount grammar

One instance answers the same upstream API at three mounts:

| Path | Mount | Narrows to |
|---|---|---|
| `/v1/messages` | aggregated | nothing |
| `/acme/v1/messages` | namespace `acme` | the exposed names under `acme/` |
| `/openai-prod/v1/messages` | provider `openai-prod` | that provider |

A **namespace** is the first segment of a slash-bearing exposed model name
(exposing `acme/fast` creates the namespace `acme`). A **provider** is a
`providers.name` — the operator's label, not the row id, because nobody types a
machine-minted id into a client's base URL. A namespace wins over a provider of
the same name.

**A prefix is only stripped when what is left is also a declared ingress
surface.** That is the rule the grammar exists for, and every simplification of
it is wrong: a provider named `backend-api` would otherwise eat
`/backend-api/codex/responses`, which is a real Codex path, and turn a data
plane call into a mount that does not exist.

**An ambiguous path therefore resolves towards the aggregated mount.** When the
first segment names something but the remainder is not a surface this gateway
serves, the path is left whole. Losing a mount is a 404 the operator can see;
taking one that was not meant is a request sent to the wrong upstream.

A mount narrows resolution by **prefixing the model name**: `/acme/v1/messages`
with `{"model":"fast"}` resolves `acme/fast`. That is the sdk resolver's own
`provider/model` grammar, so the host adds no rule of its own. A name that
already carries the prefix is left alone, so a client may spell it either way.

## The trusted-proxy rule

`x-forwarded-for` and `x-forwarded-proto` are headers, and a header is whatever
the peer wrote. They are believed **only when the socket's peer address is
loopback or is listed in `trusted_proxies`**. From any other peer they are
ignored outright — not merged, not preferred, not used as a fallback — and the
default configuration trusts nothing.

Both matter, for different reasons: a forged `x-forwarded-for` picks the address
in the operator's log next to somebody's request, and a forged
`x-forwarded-proto` picks the **scheme of the OAuth issuer identifier**, which
is a discovery document telling a client where to send an authorization code.

When the server is built without `into_make_service_with_connect_info`, the
peer is unknown, and an unknown peer is treated as untrusted.

## The lease-lifetime rule

`CallOutcome::admitted` carries the request's rate-limit charges, and a
concurrency permit measures requests **in flight**. A streamed response is in
flight long after `App::call` returned.

So the response body is a wrapper stream (`response::LeasedBody`) that **owns**
the decision: the `Admitted`, the `DownstreamCapture` and core's
`UsageCompletion` all live inside the stream. Each chunk is fed to the capture
as it passes; when the last one has been written the stream awaits the
settlement, writes the capture record and its edges, and releases the lease.
Nothing has to remember to drop anything, because there is nowhere else the
values are held. A client that hangs up mid-stream drops the body instead: the
lease is returned by `Admitted`'s own drop path and the capture is lost, which
is the same cost core's observer pays.

A websocket is the same rule with a longer clock. The pump owns the same
`Trailer` the response body does, and the socket's lease is released when the
socket closes — the `101` is not the end of anything. The socket halves are
dropped before the settlement is awaited, because core arms the exchange's
settlement inside the socket it handed over: the guard that finishes it lives
in the incoming stream, so awaiting the `UsageCompletion` while still holding
the socket waits for a value only dropping it can produce.

## Two error envelopes

The product surfaces answer

```json
{ "error": { "code": "forbidden", "message": "…" } }
```

and the OAuth endpoints answer RFC 6749 §5.2's

```json
{ "error": "invalid_grant", "error_description": "…" }
```

They are separate `IntoResponse` types, so an endpoint picks its envelope by
which wrapper it returns and the two cannot be confused at a call site. An
OAuth client cannot read the first document: the RFC says `error` is a string,
so a client that finds an object there sees no code at all and cannot tell
"re-run the login" from "keep polling".

A 5xx message is never returned to the caller — it can quote a DSN, a row or a
header. The code is; the text goes to the operator's log.

## Known limitations

- **A model-less operation on a provider mount is narrowed to that provider's
  channel, not to the provider.** `DataPlaneRequest` has a `channel` field and
  no provider field, so `GET /p1/v1/models` lists the models of every provider
  on `p1`'s channel that the caller may reach. Closing it needs a field on
  `DataPlaneRequest`.
- **Websocket upgrades: what is deliberately left out.** There is no
  subprotocol negotiation of this host's own — whatever the upstream selected
  is echoed to the client and nothing else is offered. Frames are forwarded
  unchanged; the rewrite rules core applies to them are core's. A socket has no
  idle timeout here, because core's own note says a realtime session "is
  bounded by cancellation and the frame cap, not by the HTTP idle/total
  timeouts: a realtime session legitimately waits on a user". And there is no
  per-turn capture: see above for why.
- **Sign-in is not rate limited.** `gproxy-app` explains why it cannot do it
  (it has no client address); this host has one and does not yet use it.
- **A client disconnect does not cancel the upstream call.**
  `DataPlaneRequest::cancellation` is left unset. A websocket is the exception
  by construction: a client that goes away ends the pump, which closes the
  upstream socket.

## Tests

`cargo test -p gproxy-host-axum`. Every integration test builds the real router
over an in-memory instance; only the upstream is scripted. A test that called a
handler function directly would skip the part being tested.

Most of the suite drives the router with `tower::ServiceExt::oneshot`. The
websocket round trips cannot: `oneshot` never produces hyper's `OnUpgrade`
extension, and an upgrade is made of it, so `tests/websocket.rs` binds a
loopback port and speaks the protocol with `tokio-tungstenite`. Every refusal
is still a `oneshot` test — which is itself the assertion that it never
upgraded anything.
