---
title: Pricing & Tool Billing
description: How GPROXY prices a request — token costs, mode variants, tool call billing, and how admin edits reach the billing engine.
---

Every request GPROXY handles is priced at response time and the result is
stored in the `usages.cost` column. This page documents the pricing data
model, where the values live, and how admin edits propagate into the
billing engine.

## The `ModelPrice` shape

A single JSON blob — `models.pricing_json` — is the authoritative source of
pricing for a given `(provider_id, model_id)` row. It mirrors the
`gproxy_sdk::provider::billing::ModelPrice` struct defined in
[`sdk/gproxy-provider/src/billing.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/billing.rs):

```json
{
  "price_each_call": 0.005,
  "price_tiers": [
    {
      "input_tokens_up_to": 200000,
      "price_input_tokens": 3.0,
      "price_output_tokens": 15.0,
      "price_cache_read_input_tokens": 0.3,
      "price_cache_creation_input_tokens": 3.75,
      "price_cache_creation_input_tokens_5min": 3.75,
      "price_cache_creation_input_tokens_1h": 6.0
    }
  ],
  "flex_price_tiers": [],
  "scale_price_tiers": [],
  "priority_price_tiers": [],
  "tool_call_prices": {
    "web_search": 0.01
  }
}
```

Fields:

- `price_each_call` — flat USD fee per request, regardless of tokens.
- `price_tiers[]` — per-token prices bucketed by `input_tokens_up_to`.
  All token prices are **per 1,000,000 tokens**. The first tier whose
  `input_tokens_up_to` is >= the summed input-side token count (input +
  cache read + cache creation) is selected.
- `flex_price_each_call` / `flex_price_tiers` — override for OpenAI
  `service_tier: "flex"`.
- `scale_price_each_call` / `scale_price_tiers` — override for
  `service_tier: "scale"`.
- `priority_price_each_call` / `priority_price_tiers` — override for
  OpenAI `service_tier: "priority"` and Anthropic `speed: "fast"`.
- `tool_call_prices{}` — flat USD fee per invocation, keyed by tool name.
  Currently populated for Claude `web_search` (see
  [Tool billing](#tool-billing) below).

`model_id` and `display_name` live in their own columns on the `models`
table and are **not** stored inside the JSON blob; they are stamped back
onto the parsed `ModelPrice` at load time.

## Where pricing lives

- **Built-in JSON** — each channel ships a default price table at
  [`sdk/gproxy-provider/src/channels/pricing/*.json`](https://github.com/LeenHawk/gproxy/tree/main/sdk/gproxy-provider/src/channels/pricing).
  These are compiled into the binary via `include_str!` and seeded into
  the DB on first run of each provider.
- **DB (`models.pricing_json`)** — the authoritative source at runtime.
  Admin edits write here, bootstrap seeds from the built-in JSON only
  when a row is missing.
- **In-memory `MemoryModel.pricing`** — a parsed `ModelPrice` cloned from
  the DB into the routing service on boot and on every admin mutation.
- **Billing engine** — `ProviderInstance.model_pricing` is an
  `ArcSwap<Vec<ModelPrice>>` owned by the SDK's `ProviderStore`. It is
  updated via `engine.set_model_pricing(provider_name, prices)`.

## How admin edits reach billing

When an admin upserts a model via the console or
`POST /admin/models/upsert`:

1. Handler validates `pricing_json` by parsing it into `ModelPrice`.
   Malformed JSON is rejected with `400 Bad Request` before the DB
   write.
2. `storage.upsert_model(...)` persists the row.
3. `state.upsert_model_in_memory(...)` swaps the new
   `MemoryModel.pricing` into the routing service.
4. `state.push_pricing_to_engine(provider_name)` rebuilds the per-provider
   `Vec<ModelPrice>` from the memory snapshot and calls
   `engine.set_model_pricing(...)`.
5. The next billing call (whether from the same request or an unrelated
   concurrent one) reads the new prices from the `ArcSwap`.

There is **no caching layer between admin write and billing read** — the
push is synchronous and the ArcSwap swap is lock-free. Last writer wins.

## Billing mode selection

`BillingContext.mode` is set from the request body:

| Channel           | Signal in request body                         | Mode       |
|-------------------|------------------------------------------------|------------|
| `openai`          | `service_tier: "flex"`                         | `Flex`     |
| `openai`          | `service_tier: "scale"`                        | `Scale`    |
| `openai`          | `service_tier: "priority"`                     | `Priority` |
| `anthropic`       | `speed: "fast"`                                | `Priority` |
| `claudecode`      | `speed: "fast"`                                | `Priority` |
| anything else     | —                                              | `Default`  |

When `mode` is not `Default`, the engine looks for a mode-specific tier
array (`flex_price_tiers`, etc.) on the exact model first, then on the
`default` model row. If none exists, it falls back to `price_tiers`.

## Token pricing formula

For the selected tier, each non-null price field contributes:

```
amount = tokens × unit_price ÷ 1_000_000
```

Summed across `input_tokens`, `output_tokens`, `cache_read_input_tokens`,
`cache_creation_input_tokens`, `cache_creation_input_tokens_5min`, and
`cache_creation_input_tokens_1h`.

The tier is selected by `effective_input_tokens(usage)` which is
`input + cache_read + cache_creation + cache_creation_5min + cache_creation_1h`.

## Tool billing

GPROXY bills server-side tool calls based on **actual invocation counts
reported by the upstream response**, not on whether the client declared
the tool in the request body.

For a matching entry in `tool_call_prices`, the formula is:

```
amount = usage.tool_uses[tool_key] × unit_price
```

Where `usage.tool_uses` is populated by the usage extractor in
[`sdk/gproxy-provider/src/usage.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/usage.rs).

**What is populated today:**

| Channel           | Source                                               | Tool key      |
|-------------------|------------------------------------------------------|---------------|
| `anthropic`       | `usage.server_tool_use.web_search_requests`          | `web_search`  |
| `claudecode`      | (same as anthropic; message_delta events counted)    | `web_search`  |

**What is NOT populated (yet):**

- OpenAI Responses API tool counts (`file_search`, `web_search_preview`).
  The response shape exposes them in `output[]` arrays and needs a
  per-type counter. Follow-up issue.
- Gemini `google_search`, `code_execution`, `url_context`. Gemini's
  `usageMetadata` does not surface a count; the tools fire but no
  invocation total is reported.

A tool that appears in `tool_call_prices` but is never invoked bills
nothing. A tool that's invoked without a matching `tool_call_prices`
entry bills nothing.

## Price matching: exact → `default` fallback

Price lookup is strict string matching on `model_id`:

1. Find an exact-match `ModelPrice` row where `model_id == request_model_id`.
2. If missing, fall back to a row with `model_id == "default"`.
3. If neither exists, billing returns `None` and `usages.cost` is `0.0`.

There is **no regex, prefix, or glob matching**. If you want a shared
tier across many models, define a `default` row in the pricing JSON and
let unspecified models fall back to it.

## Legacy columns

The `models` table still has `price_each_call` and `price_tiers_json`
columns — they are remnants of the pre-v1 pricing shape. The runtime
does not read or write them; they are kept only so the one-shot
`backfill_legacy_pricing_json` helper can migrate rows from older
deployments on first boot. A later release will drop them via an
explicit `ALTER TABLE`.

## Where to look when pricing is wrong

- **Expected price not applied** — check that `models.pricing_json` is
  populated for the row. Rows with `NULL pricing_json` bill `0.0`.
- **Admin edit had no effect** — check server logs for
  `push_pricing_to_engine: provider not registered in engine store` —
  that warn means the admin mutation went to the DB but the engine's
  provider store has no matching entry, usually because the provider
  was renamed after the model was created.
- **Tool was invoked but not billed** — confirm the provider channel
  actually populates `usage.tool_uses` (currently only Anthropic
  `web_search`) and that `tool_call_prices[tool_key]` is set in
  `pricing_json`.
- **Wrong tier selected** — the tier selector uses the sum of
  `input_tokens + cache_* tokens`, not `input_tokens` alone. A request
  with mostly cached prompt tokens can cross a tier boundary even
  though the billable input is small.
