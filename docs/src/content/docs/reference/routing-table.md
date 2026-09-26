---
title: "Routing & Endpoints"
description: "The complete v4 ingress table, the mount grammar, the OAuth and management routes, WebSocket surfaces, resolution and the failover budget."
---

Everything below is the native host's route table, and the Workers host mounts
the same one. `gproxy-protocol` deliberately declares no paths — which URL
serves an operation is an HTTP convention owned by the ingress layer — so this
page is that layer's table.

## The Gateway's Own Routes

Four, and everything else falls through to the data plane. That is the opposite
of the usual arrangement and it is deliberate: the gateway's own routes are a
short, known list, and everything else is somebody else's API that this
instance forwards. A new upstream surface must not require a new route here.

| Method | Path | What it is |
| --- | --- | --- |
| `GET` | `/healthz` | liveness and the published config revision; **unauthenticated** |
| `GET` | `/publications/{id}` | a published body. The id **is** the credential, so no key is asked for |
| — | `/admin/api/…` | the operator surface |
| — | `/portal/api/…` | the end user's own surface |
| — | everything else | the ingress fallback |

```sh
curl -s http://127.0.0.1:8787/healthz
```

```json
{"revision":2,"status":"ok"}
```

`/healthz` reads the published snapshot and touches neither the database nor
the cache, so a load balancer polling it cannot itself become the load that
fails it.

## The Order of Ingress

1. **CORS and the client address.** A preflight is answered here and never
   forwarded — it asks this instance what it will accept, and the upstream has
   no opinion about that. The client address is resolved under the
   [trusted-proxy rule](/reference/configuration/#the-trusted-proxy-rule)
   before anything can log it.
2. **The mount** — which slice of the path is this gateway's.
3. **The OAuth issuer**, at `{mount}/v1/oauth/…`, before the data plane because
   these paths are *this* instance's and answer in a different error envelope.
4. **A channel's vendor service route** — Codex's `/backend-api/…`, Claude
   Code's `/api/…`. Only on a provider or namespace mount: matching one needs a
   channel, and the aggregated mount names none.
5. **The data plane**, below.
6. **The console**, with an SPA fallback, behind the configuration switch.

Anything that reaches the end is a 404.

## The Data Plane

A request body is held to `settings.maxRequestBodyBytes` (50 MiB by default;
`maxUploadBodyBytes`, 512 MiB, for a file upload) before anything else happens,
authentication included.

### Content generation

| Method | Path | Operation / dialect |
| --- | --- | --- |
| `POST` | `/v1/responses` | `generate_content` / `openai` |
| `POST` | `/v1/chat/completions` | `generate_content` / `openai_chat` |
| `POST` | `/v1/messages` | `generate_content` / `claude` |
| `POST` | `/v1/messages/count_tokens` | `count_tokens` / `claude` |
| `POST` | `/v1beta/models/{model}:generateContent` | `generate_content` / `gemini` |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | `stream_generate_content` / `gemini` |
| `POST` | `/v1beta/models/{model}:countTokens` | `count_tokens` / `gemini` |
| `POST` | `/v1beta/models/{model}:embedContent` | `create_embedding` / `gemini` |
| `POST` | `/v1beta/models/{model}:batchEmbedContents` | `batch_create_embedding` / `gemini` |

The first three become `stream_generate_content` when the body says
`"stream": true`. They are **separate operations** — separate rules, separate
settlement — so the flag is read at ingress rather than left for a channel to
notice. Gemini says it in the path instead, which is why that dialect has two
rows.

### Models

| Method | Path | Operation / dialect |
| --- | --- | --- |
| `GET` | `/v1/models` | `list_models` / `openai`, then `claude` |
| `GET` | `/v1/models/{id}` | `get_model` / `openai`, then `claude` |
| `GET` | `/v1beta/models` | `list_models` / `gemini` |
| `GET` | `/v1beta/models/{id}` | `get_model` / `gemini` |

### Everything else

| Method | Path | Operation / dialect |
| --- | --- | --- |
| `POST` | `/v1/embeddings` | `create_embedding` / `openai` |
| `POST` | `/v1/moderations` | `guardian_classify` / `openai` |
| `POST` | `/v1/rerank` | `rerank` / `openai` |
| `POST` | `/v1/conversations` | `create_conversation` / `openai` |
| `POST` | `/v1/images/generations` | `create_image` / `openai` |
| `POST` | `/v1/images/edits` | `edit_image` / `openai` |
| `POST` | `/v1/audio/speech` | `create_speech` / `openai` |
| `POST` | `/v1/audio/transcriptions` | `create_transcription` / `openai` |
| `POST` | `/v1/audio/translations` | `create_translation` / `openai` |

### Files

| Method | Path | Operation / dialect |
| --- | --- | --- |
| `GET` `POST` | `/v1/files` | `list_files` / `create_file`, `openai` then `claude` |
| `GET` `DELETE` | `/v1/files/{id}` | `retrieve_file` / `delete_file` |
| `GET` | `/v1/files/{id}/content` | `retrieve_file_content` |
| `GET` `POST` | `/v1beta/files` | the Gemini spellings |
| `POST` | `/upload/v1beta/files` | `create_file` / `gemini` |
| `GET` `DELETE` | `/v1beta/files/{id}` | `retrieve_file` / `delete_file`, `gemini` |
| `GET` | `/v1beta/files/{id}:download` | `retrieve_file_content` / `gemini` |

### Video

| Method | Path | Operation |
| --- | --- | --- |
| `GET` `POST` | `/v1/videos` | `list_videos` / `create_video` |
| `GET` `DELETE` | `/v1/videos/{id}` | `retrieve_video` / `delete_video` |
| `GET` | `/v1/videos/{id}/content` | `download_video_content` |

Sora-only operations — remix, edit, extend, characters — are **deliberately
absent**. No other vendor offers them, and they come back when a second one
does.

### Realtime and sockets

| Method | Path | Operation / dialect |
| --- | --- | --- |
| `POST` | `/v1/realtime/calls` | `create_realtime_call` / `openai` |
| `GET` | `/v1/realtime` | `connect_realtime` / `openai` — upgrade |
| `GET` | `/v1/live` | the same, at the WebRTC spelling |
| `GET` | `/v1/live/{call_id}` | the continuation with the call in the path |
| `GET` | `/v1/responses/ws` | `generate_content` / `openai_responses_websocket` |
| `GET` | `/ws/v1beta/BidiGenerateContent` | `connect_realtime` / `gemini` — Gemini Live |

`POST /v1/realtime/calls` is an HTTP multipart request carrying an SDP offer,
and the handshake that continues it carries **no body at all**. What survives
the upgrade is the call id — in the path, or in `?call_id=` — which the engine
looks up to pin the socket to the credential that answered the offer. The query
is forwarded intact for exactly that reason.

A realtime session is **never converted**: only same-dialect passthrough,
because a client that can keep talking mid-response has no equivalent in a
half-duplex dialect.

**Nothing is upgraded before it is allowed.** Authentication, then the
handshake shape, then admission, then the upstream handshake; the `101` is
written last. Every refusal is therefore an HTTP answer a client can read — a
socket that is accepted and immediately closed carries no status, no body and
no code. A **refused upstream handshake is relayed verbatim**: a vendor's
`429 {"error":{"code":"insufficient_quota"}}` is worth more than any 502 this
gateway could invent.

A socket holds its concurrency lease until it **closes**, not until the `101`
was written. A session that runs for an hour holds its slot for that hour.

### The ambiguous paths

`/v1/models`, `/v1/models/{id}` and `/v1/files` are spelled identically by
OpenAI, Claude and Gemini's v1 surface, and the dialect decides which wire
types a conversion is asked for — so guessing it wrong is a converted body the
client cannot parse.

The tiebreak is **the client's own authentication header** —
`x-goog-api-key` is Gemini's, `anthropic-version` is Claude's — which is
evidence the client supplied about itself rather than a default this gateway
invented. With no evidence the first row wins, and the table is ordered so that
is OpenAI.

## The Mount Grammar

| Path | Mount | Narrows to |
| --- | --- | --- |
| `/v1/messages` | aggregated | nothing |
| `/acme/v1/messages` | namespace `acme` | the exposed names under `acme/` |
| `/openai-prod/v1/messages` | provider `openai-prod` | that provider |

A **namespace** is the first segment of a slash-bearing exposed model name. A
**provider** mount names a provider's `name` — the operator's label, not the
row id, because nobody types a machine-minted id into a client's base URL. A
namespace wins over a provider of the same name.

**A prefix is only stripped when what is left is also a declared surface.**
Every simplification of that rule is wrong: a provider named `backend-api`
would otherwise eat `/backend-api/codex/responses`, which is a real Codex path,
and turn a data-plane call into a mount that does not exist.

**An ambiguous path therefore resolves towards the aggregated mount.** When the
first segment names something but the remainder is not a surface this gateway
serves, the path is left whole. Losing a mount is a 404 an operator can see;
taking one that was not meant is a request sent to the wrong upstream.

A mount narrows by **prefixing the model name**, which is the resolver's own
grammar rather than a second rule. A name that already carries the prefix is
left alone.

:::note[One known gap]
A **model-less** operation on a provider mount is narrowed to that provider's
*channel*, not to the provider. `GET /p1/v1/models` lists the models of every
provider on `p1`'s channel that the caller may reach. Closing it needs a field
the request shape does not have yet.
:::

## The OAuth Issuer

Relative to a mount prefix (`""`, `/acme`, `/openai-prod`):

```text
GET  {prefix}/v1/oauth/authorize    consent handoff (302 to the portal for a browser)
POST {prefix}/v1/oauth/authorize    the person's decision
POST {prefix}/v1/oauth/token        code, refresh and device grants
POST {prefix}/v1/oauth/device/code
POST {prefix}/v1/oauth/revoke
GET  {prefix}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{prefix}/v1   (RFC 8414 §3.1)
```

The issuer identifier is `{origin}{prefix}/v1`, which is why the aggregated
mount is `https://host/v1` and not `https://host`: one rule then produces all
three mounts. RFC 8414 requires the identifier to be exactly the one a client
fetched the document from, and only the host knows which mount a request
arrived on, so it is passed in rather than computed.

v3 served these at the root. Moving them under `/v1` made the prefix rule the
same one `/v1/messages` follows, and the move was cheap because v3 never
implemented the discovery endpoint at all — clients were hard-coding the
address anyway.

## Management Routes

Every identity family has the same five routes, generated from one declaration
so a family cannot accidentally have four of them:

```text
GET    /admin/api/{family}           list      (page and filters in the query)
POST   /admin/api/{family}           create
GET    /admin/api/{family}/{id}      get
PATCH  /admin/api/{family}/{id}      update
DELETE /admin/api/{family}/{id}      delete    → 204
```

**Identity** families: `users`, `api-keys`, `organizations`, `teams`,
`permissions`, `rate-limits`, `subscriptions`, `pools`, `pool-members`,
`plans`, `plan-limits`. `oauth-clients` has the first four plus
`POST …/{id}/retire` instead of a delete — the grants a client issued still
name it.

**Configuration** families get the same five **plus a batch**, because every
one of them takes a batch and a batch is one revision commit however many rows
it names:

```text
POST /admin/api/{family}/batch
     [{"create": …}, {"update": {"id": …, "patch": …}}, {"delete": "id"}]
```

`providers`, `credentials`, `models`, `provider-models`, `routes`,
`route-members`, `connection-profiles`, `rule-sets`, `rules`,
`provider-rule-sets`, `operation-rules`, `operation-endpoints`, `quotas`,
`price-rules`, `price-rates`, `price-tiers`.

Beyond the five: `settings`, the credential operations (`reveal`, `status`,
`refresh`, `quota`, `quota-probe`, `quota-reset`, `health-reset`, `limits`),
`models/{discover,discover/apply,test}`, `rule-sets/{id}/rules`,
`rule-sets/{id}/rule-presets/{preset}`,
`providers/{id}/routing-defaults/reset`, `quotas/status`,
`quotas/{id}/{reset,limit-reset}`, `export`, `import`, `connectivity/test`,
`channels`, `tls-presets`, `rule-presets`, `default-model-catalog`,
`tokenizer-vocabs`, `tokenizer-auth`, `session`, `sessions` and `audit`.

Where v3 had the same operation the path is v3's, so an operator's scripts
survive. New in v4: `/connection-profiles`, `/operation-rules`, `/operation-endpoints`,
`/price-tiers`, a single `/settings` (v3 split it in two), and
`/{family}/batch` (v3 had `/batch/{entity}`). The middleware runs authenticate
→ **require the instance administrator** → same-origin for an unsafe cookie
request → the operation → an audit row for every method that is not a read. It
is a route layer, so an unknown `/admin/api/*` path is a 404 that never touches
the database.

## Portal Routes

```text
POST   /portal/api/login             name + password → session cookie and token
POST   /portal/api/logout
GET    /portal/api/context           the caller, their memberships, their features
GET    /portal/api/models            every exposed name, each marked `permitted`
GET    /portal/api/usage             their own spend
GET    /portal/api/quota             their own budget windows
GET    /portal/api/requests          their recent requests, if enabled
GET    /portal/api/sessions
GET    /portal/api/keys              (+ POST, DELETE …/{id}, POST …/{id}/rotate, GET …/{id}/secret)
GET    /portal/api/oauth-sessions    (+ DELETE …/{id})
POST   /portal/api/password
```

The guard requires an authenticated caller and **nothing more**. There is no
role check because there is nothing to check against: every portal operation is
scoped to the caller it was built from and none of them takes a user id. A
check can be forgotten; a missing parameter cannot.

`login` and `logout` sit outside the guard — the first runs before there is a
caller, and the second has to work for a session that has already expired, or a
browser is left holding a cookie it can never discard.

## Resolution and the Failover Budget

See [Models, Routes & Exposed Names](/guides/models/) for the four name forms
and the ordering. The budget:

- it is the route's own `maxAttempts`, capped by `settings.maxAttempts`;
- it is **shared across targets**: each target is granted at most as many
  attempts as it has credentials, and never more than what is left, so a plan
  cannot cost more upstream calls than its budget;
- a failure moves to the next provider **only if another provider could serve
  it** — no usable credential, a dead credential, a continuation pinned
  elsewhere, any channel or transport error, or a 401, 403, 429 or 5xx answer;
- a spent budget, a forbidden request, a cancellation, a bad conversion and a
  store or cache failure stop the call where they are;
- with no target left, **the last answer is returned as it stands**. A 429 from
  the final provider is the caller's 429, not a synthesized error.

The request body is buffered once so it can be replayed. A streaming body over
`maxRequestBodyBytes` stays a stream and the plan is cut to a single target:
a large upload is not worth reading into memory for the sake of failover.
