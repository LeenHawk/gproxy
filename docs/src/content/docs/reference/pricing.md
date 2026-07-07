---
title: Pricing
description: How v2 stores model prices, estimates quota admission cost, and settles final usage cost.
---

GPROXY v2 pricing is managed by dedicated `price_rules` records. A rule can be
provider-scoped (`provider_id`) or global, and can match a model id exactly or
by substring.

Pricing and quotas are related but separate:

- pricing describes how much one provider model costs;
- quotas describe how much an org, team, or user is allowed to spend.

If no enabled price rule matches, the request still runs and usage is recorded,
but cost is `0`. Malformed decimal price fields are rejected when rules are
written or imported.

## Price rule shape

```json
{
  "id": 1,
  "provider_id": 1,
  "match_type": "exact",
  "model_match": "gpt-4.1-mini",
  "input_price": "0.40",
  "output_price": "1.60",
  "cache_read_price": "0",
  "cache_creation_5m_price": "0",
  "cache_creation_1h_price": "0",
  "image_price": "0",
  "enabled": true
}
```

`provider_id` can be `null`. A null provider makes the rule global.

## Price fields

All price fields are decimal strings. Token prices are per 1,000,000 tokens;
image price is per generated image.

Supported fields:

| Field | Meaning |
| --- | --- |
| `input_price` | Per-million input token price. |
| `output_price` | Per-million output token price. |
| `cache_read_price` | Per-million cache-read token price. |
| `cache_creation_5m_price` | Per-million 5-minute cache-creation token price. |
| `cache_creation_1h_price` | Per-million 1-hour cache-creation token price. |
| `image_price` | Per generated image price. |

The token cost formula is:

```text
cost =
  input_tokens * input_price / 1_000_000
+ output_tokens * output_price / 1_000_000
+ cache_read_tokens * cache_read_price / 1_000_000
+ cache_creation_5m_tokens * cache_creation_5m_price / 1_000_000
+ cache_creation_1h_tokens * cache_creation_1h_price / 1_000_000
```

## Image pricing

For image operations, `image_price` is a flat per-image price. It is not per
million tokens and it is not tiered by size or quality.

## Runtime lookup

The control-plane snapshot caches enabled price rules. During admission and
settlement, GPROXY resolves pricing for `(provider_id, upstream_model_id)`.

Rule matching order is:

1. provider exact;
2. global exact;
3. provider contains;
4. global contains.

Within the same rank, the longer `model_match` wins, then lower `id` wins for
deterministic ties.

## Admission estimates

Before an upstream request is sent, quota admission uses a best-effort estimate:

- estimated input tokens are the request body length used by the current
  pending-cost estimator;
- output, cache, and image components are not estimated;
- the estimate is priced with the selected price rule's token pricing;
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

Use the console Pricing page or the price-rule admin endpoint:

```text
GET    /admin/price-rules
POST   /admin/price-rules
DELETE /admin/price-rules/{id}
```

JSON import/export uses the `price_rules` array:

```json
{
  "price_rules": [
    {
      "id": 1,
      "provider_id": 1,
      "match_type": "exact",
      "model_match": "gpt-4.1-mini",
      "input_price": "0.40",
      "output_price": "1.60",
      "cache_read_price": "0",
      "cache_creation_5m_price": "0",
      "cache_creation_1h_price": "0",
      "image_price": "0",
      "enabled": true
    }
  ]
}
```

After admin mutations, GPROXY invalidates the control-plane snapshot so new
requests see the updated pricing rules.
