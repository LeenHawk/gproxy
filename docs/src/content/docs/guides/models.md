---
title: "Models, Routes & Exposed Names"
description: "How a client model name resolves to a provider and a credential: the four forms, routes and their members, exposed names, namespaces and the model catalogue."
---

A client model name is rarely an upstream model id. v4 resolves it **before**
the engine runs, so the engine only ever sees an already-chosen target.

```text
request model
  → the first matching form: exposed name · channel/model · provider/model
  → candidates, narrowed by channel, allowed providers, allowed credentials
  → ordered by (tier, health, descending weight, stable id)
  → the leading run balanced by the route's strategy
  → one (provider, credential, upstream model) per attempt
```

There are **no aliases and no variant suffixes** in v4. Both existed in v3 and
neither was ported: an alias was a second name-rewriting stage in front of a
name-rewriting stage, and a variant suffix was request shaping hidden inside
resolution. Request shaping is a rewrite rule now — visible, ordered, and
filtered by model — see [Rewrite Rules](/guides/rules/).

## The Four Forms

A name matches the first rule that applies.

| Name | Resolves to | Attempt budget |
| --- | --- | --- |
| absent | every enabled provider, no upstream model | `settings.max_attempts` |
| an **exposed model name** | that route's enabled members | the route's own |
| `channel/model` | the providers of that channel, preferring the ones whose catalogue lists `model` | `settings.max_attempts` |
| `provider/model` | that one provider | `settings.max_attempts` |
| anything else | `404 unknown_model` | — |

Exposed names are matched **exactly and first**, so an operator can expose the
literal name `openai/gpt-5` as a public name of their own.

Inside the prefix forms, **a channel id beats a provider of the same name**. A
channel id is fixed by the build and cannot be renamed out of the way; a
provider always can. The alternative is worse: name a provider `codex` and all
`codex/*` traffic could never reach the `codex` channel again, with no way
around it.

## Routes

A route is a named pool with its own balancing strategy and attempt budget.

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/routes \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"name":"main","strategy":"round_robin","maxAttempts":6}'
```

```json
{"id":"33a88261f571347c7f0408c3bd2e2164","name":"main","strategy":"round_robin",
 "maxAttempts":6,"enabled":true}
```

| Field | Meaning |
| --- | --- |
| `name` | Unique. A route name is not addressable on its own — only an exposed name reaches it. |
| `strategy` | `round_robin`, `weighted` or `failover`. |
| `maxAttempts` | The total attempt budget including the first call. `settings.maxAttempts` (default 6) is a hard ceiling on it at execution time. |

## Members

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/route-members \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"routeId":"…","providerId":"…","upstreamModel":"gpt-4o-mini",
       "tier":0,"weight":100}'
```

| Field | Meaning |
| --- | --- |
| `providerId`, `upstreamModel` | Where this member sends traffic. The model name is an explicit string, not a catalogue foreign key. |
| `tier` | Lower is preferred. Tier 0 is exhausted before tier 1 sees any traffic. |
| `weight` | Positive, default 100. Splits traffic inside a tier, and orders failover candidates. |
| `enabled` | A disabled member leaves the plan. |

A member names no credential. Which credential inside the chosen provider
serves the call is a separate decision the engine makes, and it fails over
between that provider's credentials before the plan moves on to the next
member.

### How the order is decided

Candidates sort by `(tier, health, descending weight, stable id)`.

**Tier is a hard preference.** Only the leading run — the candidates sharing
the first one's tier *and* health — is balanced, and then by the strategy:

| Strategy | Effect on the leading run |
| --- | --- |
| `round_robin` | rotates it on a per-route counter |
| `weighted` | promotes the smooth weighted pick to the front |
| `failover` | leaves it alone: the sorted order *is* the answer |

Rotation is a counter, never a random draw, so wasm and native behave
identically and a sequence is reproducible.

Disabled, retired and dead credentials are dropped outright. A blocked
credential is dropped while its provider still has an unblocked one; a provider
whose credentials are **all** blocked keeps them and sorts behind every healthy
provider. A rate limit is a last resort, not an outage.

A name that resolved but reaches nothing is a different error from a name that
was never known — a configuration problem rather than an unknown model.

## Exposed Names

An exposed model is the public name a client sends. It is what stops clients
naming your infrastructure.

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/exposed-models \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"routeId":"…","name":"fast"}'
```

Many names may expose one route. A name is globally unique and matched exactly.

### Namespaces

A name with a `/` in it derives a namespace at runtime: exposing `acme/fast`
makes `acme` a mount, and `/acme/v1/chat/completions` with `{"model":"fast"}`
resolves `acme/fast`.

```sh
curl -s http://127.0.0.1:8787/acme/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"hi"}]}'
```

A namespace is a **name index**, not a stored group and not an ownership
scope. Nothing is created and nothing is owned by it.

### Reserved first segments

An exposed name whose first segment is a registered channel id or an existing
provider name could never be reached — the prefix forms would claim it first —
so the write is refused rather than left to fail silently at runtime:

```json
{"error":{"code":"invalid_request","message":"invalid request: `codex/` is reserved:
 a first segment naming a channel or a provider already means `channel/model` or
 `provider/model` narrowing, so `codex/fast` could never reach its route"}}
```

Anything else is fine. `coding/fast` is a perfectly good public name.

## The Model Catalogue

Two tables, and neither is required for routing.

**`provider_models`** records which upstream names a provider serves:

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/provider-models \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","upstreamName":"gpt-4o-mini"}'
```

```json
{"id":"f32df0bb6378c03e70f7c3aa6315a3b8","providerId":"5a45fd807be0…",
 "upstreamName":"gpt-4o-mini","modelId":null,"metadata":{},"enabled":true}
```

It is what the `channel/model` form prefers when it chooses among a channel's
providers, and it is where discovery writes. **`models`** is the global
catalogue a `provider_models` row may point at: one name, its metadata, and the
tokenizer vocabulary token estimation should use for it.

Routing can match names absent from both. A route member names its upstream
model as a plain string, so a catalogue row is documentation and pricing
material, not a prerequisite.

### Filling it from the upstream

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/models/discover \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…"}'
```

Discovery asks in the provider's **own dialect**, so nothing is converted and
the names are the upstream's. Each answer carries whether this provider already
has a row for it and whether the bundled catalogue can price it.
`POST /admin/api/models/discover/apply` inserts the ones you name, skipping
those already there, so applying a discovery twice is the same as once.

Both this and `POST /admin/api/models/test` spend a real credential and write a
real usage row. See
[the two probes](/guides/providers/#the-two-probes).

The bundled catalogue — names, context windows and default prices this release
knew about — is a snapshot taken when the asset was generated, not a live
directory:

```sh
curl -s http://127.0.0.1:8787/admin/api/default-model-catalog -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/default-model-catalog/apply-prices \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","modelIds":["gpt-4o-mini"],"overwrite":false}'
```

`overwrite: false` is what makes re-applying safe: a rule an operator edited
keeps its edit and is reported as skipped.

## What a Caller Sees

`GET /v1/models` is **forwarded to a provider** and answers with that
upstream's own catalogue. The list of names *you* publish is the portal's, and
it omits nothing:

```sh
curl -s http://127.0.0.1:8787/portal/api/models -H "Authorization: Bearer $GPROXY_KEY"
```

```json
[{"name":"custom/gpt-4o-mini","providerCount":1,"channelIds":["custom"],"permitted":true},
 {"name":"fast","providerCount":1,"channelIds":["custom"],"permitted":true}]
```

A name the caller's rules do not reach stays in the list with
`permitted: false`. v3 dropped such rows; v4 does not, because a list that
silently omits makes "this model 404s" and "you are not allowed this model" the
same observation — and there is nothing to protect, since an exposed name is
instance configuration the operator publishes anyway.

What *is* withheld is the provider ids behind a name: the answer reports a
count and a channel, which say how redundant a name is without naming the
machinery. The `provider/model` form also resolves and is deliberately **not**
listed — its left half is a renameable row, and printing it would hand users a
name that stops working when somebody edits a provider.

### Global models and default metadata

The Console's **Models** (`/console/model-catalog`) lists bundled models, context and output limits, modalities, supported parameters, reference rates and pricing tiers. Search the catalog, save local metadata overrides or add your own models. Removing a local entry leaves its bundled model visible.

Discovery matches the full model ID first, then a unique basename; ambiguous names receive no defaults. Merge order is bundled defaults → non-null upstream fields → local overrides. Console imports fill only missing fields on existing provider models. Metadata is copied at import time; catalog edits do not automatically change previously imported provider rows.

Default prices are an OpenRouter snapshot, not every provider's contract. Applying default prices creates global pricing rules without replacing an existing rule with the same pattern. The price editor manages rates, context tiers and service tiers. Provider-specific rules take precedence over global rules.

Refresh the bundled asset with `node scripts/update-openrouter-model-catalog.mjs`, or use `--input response.json` for an offline refresh. The public model endpoint needs no key; an optional credential is read only from `OPENROUTER_API_KEY`. The generator retains raw `source_pricing` for review and maps only supported billing units. Missing or dynamic prices do not mean free. Before supplementing from official price pages, verify the exact model version, region, unit and threshold rather than guessing a missing rate.
