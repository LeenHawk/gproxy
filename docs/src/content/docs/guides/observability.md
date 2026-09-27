---
title: Usage, Logs & Audit
description: "What GPROXY v4 records: usage rows and their settlement, downstream and upstream captures, redaction, the audit trail, and what the HTTP host does not yet expose."
---

Everything below is written to the database, so every instance sharing one
database shares one view.

Three things are recorded, and they are separately switchable **at runtime**,
not at compile time — an operator needs to decide per deployment, and a Cargo
feature cannot give them that.

| | The expensive part | Turning it off costs you |
| --- | --- | --- |
| settlement | usage extraction, token estimation, pricing | quota accounting **and** cost statistics, together |
| capture | cloning the body — the most expensive of the three | any request or response retained |
| tracing | string formatting | everything but error logs |

The switch is asked **before** the work, never applied to a result afterwards.
v3 cloned the body and then let a sink decide, so an empty sink saved nothing;
v4 asks first, and with capture off nothing is allocated and no body is copied.

## Usage Records

Every settled exchange writes one row. Settlement happens on **every** path
that reaches an upstream — there is no fast path around the funnel, because a
path around it is unmetered traffic.

A row carries its request id, the model, the operation, the settled cost, and a
`metrics` document holding the normalized token counts, the per-exchange
breakdown behind them, the settlement state and the priced amounts.

A request that reached two providers has **two exchanges in one row**, each
with that attempt's own tokens and price. Grouping by provider is therefore
per-exchange rather than per-row: a failed-over request counts under both.

```json
{"dimensions":{"estimated":"true","unpriced":"true"},
 "exchanges":[{"attempt_id":"1a0c3372ac7-0-1","attempt_ordinal":1,
   "credential_id":"b8aad67f…","model":"gpt-4o-mini","provider_id":"5a45fd80…"}]}
```

Two dimensions are worth knowing by name:

- **`estimated = true`** — the upstream reported no usage and GPROXY counted
  the tokens itself. A token count is `null` rather than `0` when a field was
  never reported: an upstream that did not measure something did not measure
  zero.
- **`unpriced = true`** — no price rule covered this model. The request still
  settles, at zero, because the operator wanted the signal that a model is
  being served for free rather than a refusal.

A **cancelled** request is still a metered request: it gets its row like any
other, with the state set to `cancelled`, whatever the upstream managed to
report before it was stopped.

### Reading it back

```sh
curl -s http://127.0.0.1:8787/portal/api/usage -H "Authorization: Bearer $GPROXY_KEY"
curl -s 'http://127.0.0.1:8787/portal/api/usage?groupBy=provider' -H "Authorization: Bearer $GPROXY_KEY"
```

```json
{"summary":{"requests":19,"inputTokens":43,"outputTokens":3830,
  "cachedInputTokens":0,"cacheCreationTokens":0,"reasoningTokens":0,
  "cost":"0","currency":null,"truncated":false,"scanned":19},
 "groups":[…],"trend":[]}
```

`currency` is `USD` when any record was priced, otherwise `null`.
Fixed token counts and media/tool quantities are stored in dedicated columns;
`quantities` returns media/tool and custom counters as exact decimal strings.
Upstream usage rows carry their own provider, credential and cost independently
of logs. Provider and credential filters run in SQL.

Aggregates are folded in Rust over a **scan cap** — 50,000 matching downstream
records by default. `truncated` and `scanned` report whether the cap stopped the
read, rather than presenting a partial total as complete.

:::caution[The operator's read side is not on HTTP yet]
`gproxy-sdk` has a full query side — usage records, summaries, groups, trends,
quota windows and settlements, request logs and their details — and the desktop
host exposes all of it over IPC. **The axum host does not.** There is no
`GET /admin/api/usage` and no `GET /admin/api/logs`; both answer 404.

What the HTTP host does serve is `/portal/api/usage`, `/portal/api/quota` and
`/portal/api/requests`, which are scoped to the calling identity by
construction, plus `/admin/api/quotas/status` and `/admin/api/audit`.
:::

## Quotas and Budgets

A budget is a `quotas` row whose metric is `cost` in USD, against an owner —
`api_key`, `user`, `subscription`, `team` or `org`.

Admission hands the engine the caller's **budget chain**:
`[api_key?, user, subscription?, team?, org?]`, skipping what is not set. The
engine matches those kinds **verbatim** and knows no hierarchy between them, so
**every** enabled budget of **any** owner in the chain applies and all of them
must have room. The order is what a log reports, not a precedence.

Windows open lazily, on an `INSERT … ON CONFLICT DO NOTHING` so concurrent
instances converge on the same row, and an expired window is never deleted —
the next request simply opens the next one, and history is the full list.

```sh
curl -s 'http://127.0.0.1:8787/admin/api/quotas/status?owners=user:alice,team:t1' \
  -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/quotas/{id}/reset \
  -H "Authorization: Bearer $GPROXY_KEY"
```

A reset closes the open window at *now*, re-anchors the quota, and opens a
fresh one. History is kept.

**A budget can be overrun by at most one request.** Cost is not known until the
exchange ends, and that is the accepted price of not estimating, not
pre-charging and not rolling back. Settlement is idempotent per request id, so
a replay does not double-charge, and a settlement failure loses the accounting
rather than the delivered response.

## Captures

A capture has two sides with two owners. The engine writes the `upstream` row
for every physical send; only a host sees the inbound HTTP exchange, so the
`downstream` row is the product layer's.

| | upstream | downstream |
| --- | --- | --- |
| gate | `enableUpstreamLog` | `enableDownstreamLog` |
| body gate | `enableUpstreamLogBody` | `enableDownstreamLogBody` |
| id | its own, opaque | **the request id**, which is also the usage row's |
| body storage | streamed into capture events | the inline column, buffered and capped |
| websocket frames | one event per frame | one event per frame, buffered |

```sh
curl -s -X PATCH http://127.0.0.1:8787/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"logging":{"enableUpstreamLogBody":true}}'
```

All four switches are read off the settings row of the **revision the request
pinned**, so a request cannot be half-captured under two configurations.

With a body switch off, the body is not copied and the record's body state says
`notCaptured` — which is how a reader tells "there was no body" from "we chose
not to keep it". Two rows with zero bytes and different states are different
facts. What is kept is capped at 64 KiB per direction, and a body over it is
stored truncated with the state `partial`.

A **capture failure is never a request failure.** The request has been answered
by then, and an error in describing it must not become a response.

### A socket is one record and a list of frames

A WebSocket is **one** record with `101` as its status and an ordered event
list across both directions, not one record per exchange. Ping, pong and close
are recorded too, and a keepalive stays on the side it was sent on because it
measures the link it travelled.

No per-turn record is written and the turn column is left unset. A turn is a
dialect's notion — OpenAI's `response.created`/`response.done`, Gemini Live's
own — and the host forwards realtime frames as opaque passthrough. Inventing a
boundary the wire did not draw would put a record in the log that nothing
produced.

### Retries are edges, not columns

A capture record's provider, credential and metrics columns stay **unset** on a
downstream row. A retried request reached two providers with two credentials,
and a single column would have to pick one. The edges answer that without
picking, and the billed usage is the usage row.

Those edges are merged from two sources, because neither is complete on its
own: what the engine could **meter** (a 503 that was retried elsewhere carries
no usage and is not in the report at all), and every upstream record whose
initiator is this request (which misses one a *continuation* reached). Nothing
is written until the exchange ends; the record and all of its edges go in one
batch.

## Redaction

Redaction happens **at write time**, not at read time. A secret that reaches
the column has already leaked into every backup and replica, so nothing on the
read side redacts and a host must not assume a second pass happens: if it is in
the database, the policy allowed it there.

Header names, query parameters and JSON fields on the standard list —
`authorization`, `cookie`, `api_key`, `access_token` and their peers — are
masked, and both sides of one request hide the same things. A request body is
redacted **before** it is cut, so length is never a way past the policy.

A body that is not JSON has no key to match on and is stored as received. That
is one more reason the body switches are off by default.

`disableLogRedaction` is the explicit cleartext override.

## The Audit Trail

Every `/admin/api` call that is not a read writes one row. The action is
derived from the **matched route**, so a new route cannot forget to name
itself:

```sh
curl -s 'http://127.0.0.1:8787/admin/api/audit?limit=4' -H "Authorization: Bearer $GPROXY_KEY"
```

```json
{"items":[{"id":"d052d378c1fc61c9fe4a4738d61fc996",
  "actorUserId":"5d1eb28a…","actorApiKeyId":"29371cab…","sourceIp":null,
  "action":"admin.settings.update","entityKind":null,"entityId":null,
  "outcome":"ok","detail":{"status":200},"createdAtMs":1789982332295}]}
```

Reads are not audited, which is exactly why the two deliberate disclosures —
revealing a credential secret and revealing the tokenizer token — are `POST`s.

The row is written **outside** the revision transaction. That is not laziness:
a trail insert that failed would otherwise roll back the operation it was only
describing, and a *rejected* operation — the row an investigation actually
wants — has no transaction to join at all. A failure to write the trail is
logged, never propagated.

Caller-supplied JSON only enters an entry through a redacting constructor, and
a failure records the error's stable **code**, never its message, because a
message can quote a value the caller sent. Field names are matched whole after
normalizing case, underscores and dashes, so `passwordPolicy` and
`keyboardLayout` survive while `password` does not, and a matched value is
**replaced** with `[redacted]` rather than removed — a reader can then tell
"this operation carried a password" from "this one did not".

Queries page newest first, ordered on `(createdAtMs DESC, id)`, so two rows
written in the same millisecond cannot repeat or skip across a page boundary.

Sessions and audit rows deliberately do **not** bump the configuration
revision. Neither is in the identity snapshot — authentication reads sessions
on every request — so bumping would make every login and every audited
operation invalidate the whole fleet's snapshot for a change no snapshot
contains.

## Process Logs

```sh
gproxy serve --log-format json --log-filter 'gproxy=debug,info'
```

`--log-format` is `text` or `json`; `--log-filter` takes `RUST_LOG` syntax and
falls back to `RUST_LOG`, then to `info`.

Logs go to standard **error**, and the first-run administrator block goes to
standard output. That is what keeps the secrets out of a journal, and what
keeps `gproxy export --out -` clean.

## Request Ids

A request's id joins its usage row, its captures and its edges. There is **no
`x-request-id` response header** in v4; the id is recorded, not returned.
