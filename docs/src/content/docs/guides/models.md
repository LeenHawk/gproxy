---
title: "Models & Routes"
description: "How a client model name resolves to a provider and a credential: the four forms, routes and their members, namespaces and the model catalogue."
---

A client model name is rarely an upstream model id. v4 resolves it **before**
the engine runs, so the engine only ever sees an already-chosen target.

```text
request model
  → the first matching form: route name · channel/model · provider/model
  → candidates, narrowed by channel, allowed providers, allowed credentials
  → ordered by (tier, health, descending weight, stable id)
  → the leading run balanced by the route's strategy
  → one (provider, credential, upstream model) per attempt
```

Use a route name as the client-facing model name. To change request parameters, use [rewrite rules](/guides/rules/).

## The Four Forms

A name matches the first rule that applies.

| Name | Resolves to | Attempt budget |
| --- | --- | --- |
| absent | every enabled provider, no upstream model | `settings.max_attempts` |
| a **model route name** | that route's enabled members | the route's own |
| `channel/model` | the providers of that channel, preferring the ones whose catalogue lists `model` | `settings.max_attempts` |
| `provider/model` | that one provider | `settings.max_attempts` |
| anything else | `404 unknown_model` | — |

Route names are matched **exactly and first**. Management rejects names whose
first segment conflicts with a registered channel or provider prefix.

Inside the prefix forms, **a channel id beats a provider of the same name**. A
channel id is fixed by the build and cannot be renamed out of the way; a
provider always can. The alternative is worse: name a provider `codex` and all
`codex/*` traffic could never reach the `codex` channel again, with no way
around it.

## Routes

A route is a public model name with provider/model members, a balancing strategy
and an attempt budget. Creating `main` makes `model: "main"` address that route.

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
| `name` | Globally unique. Clients send this exact name as `model`. |
| `sessionAffinity` | Reuse a successful provider/model target for the session; defaults off. Credential selection remains the provider's responsibility. |
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

## Route Names

Create and edit names directly on routes; no separate public-name mapping is required.

### Namespaces

A name with a `/` in it derives a namespace at runtime: creating route `acme/fast`
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

A route name whose first segment is a registered channel id or an existing
provider name is refused to avoid ambiguous namespace/prefix routing:

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

`GET /v1/models` returns the configured catalogue: route names and
`provider/model`, filtered to names the caller may use. It does not rotate
between upstream directories. Add or import provider models to publish them here.
The prefix is the configured provider route name; `channel/model` remains callable
but is not automatically added as a second listing.

The two forms are equivalent: base URL `/provider-A/v1` with model `gpt-5`, or
base URL `/v1` with model `provider-A/gpt-5`. `GET /provider-A/v1/models` queries
only provider A and keeps its original model names.

The portal also includes names the caller cannot use, marked with `permitted`:

```sh
curl -s http://127.0.0.1:8787/portal/api/models -H "Authorization: Bearer $GPROXY_KEY"
```

```json
[{"name":"provider-A/gpt-4o-mini","providerCount":1,"channelIds":["custom"],"permitted":true},
 {"name":"fast","providerCount":1,"channelIds":["custom"],"permitted":true}]
```

A name the caller's rules do not reach stays in the list with
`permitted: false`. v3 dropped such rows; v4 does not, because a list that
silently omits makes "this model 404s" and "you are not allowed this model" the
same observation — and there is nothing to protect, since an exposed name is
instance configuration the operator publishes anyway.

What *is* withheld is the provider ids behind a name: the answer reports a
count and a channel, which say how redundant a name is without naming the
machinery. `provider/model` uses the provider display name; renaming a provider
changes its model prefix and base URL.

### Global models and default metadata

The Console's **Models** (`/console/model-catalog`) lists models saved in the database, using the same `/admin/api/models` endpoint as permission selectors. An empty database returns an empty list. **Load default models** imports selected entries from the bundled model table and keeps existing models unchanged; loading model metadata does not change pricing rules. Models can also be added manually or imported from OpenRouter. Deleting a model removes it from the list; it is not restored from bundled data during reads. The page shows saved capabilities and limits, provider associations, and bundled reference prices where available.

Bundled model IDs are unqualified names, such as `claude-sonnet-4`, rather than `anthropic/claude-sonnet-4`. Source URLs and raw prices retain their provenance. Qualified upstream names still resolve by basename. Default price matching prefers the longest matching model fragment; provider price imports keep the provider's actual upstream name as the rule pattern. Prices are edited in the existing global-model and provider-model dialogs.

Discovery matches the full model ID first, then a unique basename; ambiguous names receive no defaults. Merge order is bundled defaults → non-null upstream fields → local overrides. Console imports fill only missing fields on existing provider models. Metadata is copied at import time; catalog edits do not automatically change previously imported provider rows.

Default prices are an OpenRouter snapshot, not every provider's contract. Applying default prices creates global pricing rules without replacing an existing rule with the same pattern. The price editor manages rates, context tiers and service tiers. Provider-specific rules take precedence over global rules.

Refresh the bundled asset with `node scripts/update-openrouter-model-catalog.mjs`, or use `--input response.json` for an offline refresh. The public model endpoint needs no key; an optional credential is read only from `OPENROUTER_API_KEY`. The generator retains raw `source_pricing` for review and maps only supported billing units. After unit conversion, billing prices are rounded to nine decimal places using nearest rounding with ties to even; importing default prices uses the same rule. Missing or dynamic prices do not mean free. Before supplementing from official price pages, verify the exact model version, region, unit and threshold rather than guessing a missing rate.
