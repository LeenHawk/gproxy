---
title: Pricing
description: How v2 stores model prices, estimates quota admission cost, and settles final usage cost.
---

GPROXY v2 pricing is per provider model. The authoritative configuration is
`provider_models.pricing_json`; there is no separate price table.

Pricing and quotas are related but separate:

- pricing describes how much one provider model costs;
- quotas describe how much an org, team, or user is allowed to spend.

An unpriced model still runs. Missing, null, or malformed pricing fields parse
as zero, so usage is recorded but cost is `0`.

## `pricing_json` shape

Token prices are per 1,000,000 tokens. String decimals are preferred because
money is handled with decimal arithmetic, but JSON numbers are accepted.

```json
{
  "input": "3.00",
  "output": "15.00",
  "cache_read": "0.30",
  "cache_creation": "3.75"
}
```

Supported keys:

| Key | Meaning |
| --- | --- |
| `input` | Per-million input token price. |
| `output` | Per-million output token price. |
| `cache_read` | Per-million cache-read token price. |
| `cache_creation` | Per-million cache-creation token price. |
| `image` | Either a flat per-image price or a tier object for image operations. |

The token cost formula is:

```text
cost =
  input_tokens * input / 1_000_000
+ output_tokens * output / 1_000_000
+ cache_read_tokens * cache_read / 1_000_000
+ cache_creation_tokens * cache_creation / 1_000_000
```

## Image pricing

For image operations, `image` can be a scalar per-image price:

```json
{ "image": "0.04" }
```

It can also be a tier object. Lookup order is:

1. `"{size}/{quality}"`;
2. `"{size}"`;
3. `"default"`;
4. zero if no tier matches.

```json
{
  "image": {
    "1024x1024": "0.04",
    "1792x1024/hd": "0.12",
    "default": "0.02"
  }
}
```

Image pricing is per generated image, not per million tokens.

## Runtime lookup

The control-plane snapshot caches provider models by provider id. During
admission and settlement, GPROXY resolves pricing by exact
`(provider_id, upstream_model_id)` lookup in that snapshot and parses the
model's `pricing_json`.

There is no glob, prefix, or `"default"` model fallback in the current v2
pricing lookup. Configure pricing on each provider model row that should bill
non-zero cost.

## Admission estimates

Before an upstream request is sent, quota admission uses a best-effort estimate:

- estimated input tokens are the request body length used by the current
  pending-cost estimator;
- output, cache, and image components are not estimated;
- the estimate is priced with the selected provider model's token pricing;
- if the estimate is zero, pending quota pre-deduct is skipped.

For quota-bearing scopes, GPROXY adds the estimated micro-dollar cost to cache
keys named like `qp:{scope}:{id}`. These pending counters have a 15-minute TTL
so a crash between charge and refund self-heals.

## Settlement

Successful content-generation responses settle exactly once:

- non-streaming and fully buffered responses settle inline;
- native streaming responses attach a guard so normal end, upstream interruption,
  or client drop all settle once;
- if upstream usage is present in the response, it is used;
- otherwise GPROXY falls back to local counting where the compiled feature set
  supports it.

The settled request writes a `usages` row with token counts, source, end state,
latency, route/provider/user dimensions, and cost. Quota reconciliation then:

1. refunds the exact pending micro-dollar estimate;
2. atomically increments `quotas.cost_used` for each quota-bearing scope by the
   actual settled cost.

Embedding and image operations have their own provider-shaped settlement path.
Model list/get, token-count, compact, and conversation operations are not
currently billed by the content-generation settlement path.

## Where operators edit prices

Use the console or the provider-model admin endpoint:

```text
GET  /admin/providers/{provider_id}/models
POST /admin/providers/{provider_id}/models
```

JSON import/export uses the same `provider_models` input shape:

```json
{
  "id": 1,
  "provider_id": 1,
  "model_id": "gpt-4.1-mini",
  "display_name": "GPT-4.1 mini",
  "pricing_json": {
    "input": "0.40",
    "output": "1.60"
  },
  "variants_json": null,
  "enabled": true
}
```

After admin mutations, GPROXY invalidates the control-plane snapshot so new
requests see the updated model and pricing rows.
