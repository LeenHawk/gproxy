---
title: "Pricing & Tiers"
description: "price_rules, price_rates and tiers_json, the metrics settlement emits, how tiers compose, admission versus settlement, and estimation when usage is missing"
---

Pricing answers one question at settlement: what did this exchange cost,
given the provider, the upstream model, and the normalized usage the
channel extracted. It is data. A `price_rules` row selects the model, its
`price_rates` rows price each metric, and its `tiers_json` adjusts the
token ladder by prompt size and service tier. Quotas
([Permissions, Rate Limits & Quotas](/guides/permissions/)) spend the
result. Costs are decimals without a currency; every price shares
whatever unit you enter.

A fresh store loads the embedded global price catalog. Edit prices at
console → Pricing, or through `/admin/api/price-rules`,
`/admin/api/price-rates` and
`POST /admin/api/default-model-catalog/apply-prices`.

## Price Rules

| Field | Meaning |
| --- | --- |
| `provider_id` | Scope. `null` is a global rule. |
| `model_pattern` | Matched against the upstream model id. `*` matches any run of characters; everything else is literal. |
| `tiers` | The `tiers_json` array described below, or `null`. |
| `priority` | Lower first; ties break on `id`. |
| `enabled` | Disabled rules are skipped. |

Resolution for `(provider, upstream_model)`: the first matching enabled
rule scoped to that provider, else the first matching global rule. A rule
without any `price_rates` row is ignored entirely. When nothing matches,
the request still runs, usage is recorded, the cost is `0`, and the log
says `pricing missing; settling at zero cost`.

## Price Rates

| Field | Meaning |
| --- | --- |
| `rule_id` | Owning rule. |
| `metric` | A usage metric name (below). |
| `unit_size` | Positive integer. The effective rate is `price / unit_size` per unit of the metric. |
| `price` | Non-negative decimal string. |
| `conditions` | Optional object of `dimension: value` (string, number or boolean scalars). |
| `priority` | Order among rows of the same metric; lower first. |

`input_tokens`, `output_tokens` and `cached_input_tokens` rows define the
base token ladder and are read as per-million rates
(`price × 1,000,000 / unit_size`). Every other metric is priced as
`amount × price / unit_size`. For one metric, conditional rows are tried
in `(priority, id)` order and the first whose conditions all equal the
settlement's dimensions wins; otherwise the first unconditional row
applies and later unconditional duplicates are ignored.

```json
[
  { "rule_id": 1, "metric": "input_tokens", "unit_size": 1000000, "price": "0.40", "conditions": null, "priority": 0 },
  { "rule_id": 1, "metric": "output_tokens", "unit_size": 1000000, "price": "1.60", "conditions": null, "priority": 0 },
  { "rule_id": 1, "metric": "image_outputs", "unit_size": 1, "price": "0.04",
    "conditions": { "quality": "hd", "size": "1024x1024" }, "priority": 0 }
]
```

## Metrics and Dimensions

Channels extract a `NormalizedUsage` from each response:

```rust
pub struct NormalizedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub metrics: BTreeMap<String, Decimal>,
    pub dimensions: BTreeMap<String, String>,
}
```

The three token fields are columns in `usage_rows`; everything else is a
metric or a dimension. The metric names the console catalog knows:

| Metric | Unit |
| --- | --- |
| `cache_creation_5m_tokens`, `cache_creation_30m_tokens`, `cache_creation_1h_tokens` | tokens, per million, tier-aware |
| `image_output_tokens` | tokens, per million, tier-aware |
| `reasoning_tokens`, `audio_input_tokens`, `cached_audio_input_tokens`, `audio_output_tokens`, `image_input_tokens`, `video_input_tokens`, `video_tokens` | tokens |
| `search_units`, `web_searches`, `web_fetches`, `image_outputs`, `video_outputs` | counts |
| `audio_seconds`, `video_seconds` | seconds |

Dimensions observed in settlement are `service_tier` (also `speed`) and,
for image operations, `size`; a channel may add more, and any dimension
can appear in `conditions`. A metric outside this list is still priced as
long as a rate names it; the catalog is a convenience for the editor, not
a filter. `cached_input_tokens` is capped at `input_tokens`; the uncached
remainder is priced at the input rate and the cached part at the cached
rate, which falls back to the input rate when no cached rate exists.

## Tiers

`tiers_json` is an array of rows. Each row must set `service_tier`,
`min_prompt_tokens`, or both.

| Field | Meaning |
| --- | --- |
| `service_tier` | Tier name. Normalized to lowercase with `-` as `_`; `fast` → `priority`, `ultra_fast` → `ultrafast`, `default` and `on_demand` → `standard`. The catalog offers `standard`, `priority`, `flex`, `scale`, `ultrafast`, `batch`, `reserved`; other names are accepted. |
| `min_prompt_tokens` | Threshold on prompt tokens (`input_tokens` plus every `cache_creation_*_tokens`). Defaults to `0`. |
| `multiplier` | Decimal applied to the categories the row does not price explicitly. |
| `input_price`, `output_price`, `cache_read_price`, `cache_creation_5m_price`, `cache_creation_30m_price`, `cache_creation_1h_price`, `image_output_price` | Explicit per-million prices for that category. |

Composition, per category:

1. **Base ladder.** Among rows without `service_tier`, take the one with
   the highest `min_prompt_tokens` not above the prompt size. Its explicit
   prices replace the base rates for the categories it sets.
2. **Service tier.** The tier is the one the upstream reported
   (`dimensions["speed"]` or `dimensions["service_tier"]`), else the one
   the request asked for (`speed`, `service_tier` or `serviceTier` in the
   body). Among rows with that tier, take the highest threshold reached.
3. **Price.** An explicit price in the service row wins. Otherwise the
   base-ladder price is multiplied by the row's `multiplier` (default 1).

The rule has one trap: an explicit tier price replaces the whole base
ladder for that category, including long-context steps it did not
declare. With base input `1`, a base step `≥ 200,000 → 2`, and 300,000
prompt tokens:

| `batch` row | Effective input rate |
| --- | --- |
| `{"service_tier": "batch", "multiplier": "0.5"}` | `2 × 0.5 = 1` |
| `{"service_tier": "batch", "input_price": "0.5"}` | `0.5` — the 200k step is lost |
| `{"service_tier": "batch", "min_prompt_tokens": 200000, "input_price": "1"}` | `1` |

Repeat an explicit tier price at every threshold it must cover, or use a
multiplier. The console flags a missing step.

Requested versus served: admission prices the tier the request asked for;
settlement re-reads the tier from the response (top-level or under
`usage`, `usageMetadata`, `response`, `message`, or Gemini's
`x-gemini-service-tier` header) and charges that one. A `fast` request
that the provider downgrades to `default` settles at the `standard` row.

## Worked Examples

From `crates/gproxy-core/src/tests/pricing.rs`:

- Base input `1`, output `2`; base steps `≥ 100 → input 2` and
  `≥ 500,000 → input 3`. Usage 1,000,000 input and 1,000,000 output. The
  500,000 step applies: `1 × 3 + 1 × 2 = 5`.
- Base input `1`, output `2`, cached `0.5`; requested tier `priority`;
  rows `{min 1 → input 3}` and
  `{priority, min 2,000,000, multiplier 2, output 7, image_output 11}`;
  an `image_output_tokens` rate of `4` per million. Usage 2,000,000 input
  of which 1,000,000 cached, 1,000,000 output, 1,000,000 image output
  tokens. Uncached `1M × 3 × 2 = 6`, cached `1M × 0.5 × 2 = 1`, output
  `1M × 7 = 7`, image `1M × 11 = 11`: total `25`.
- Rows `{priority → input 10}` and `{standard → input 4}`; the request
  says `"service_tier": "fast"` and the response header says `default`.
  Admission estimates at `10` per million; settlement charges `4`.

## Quotas: Admission, Then Settlement

Admission runs only for billable operations (settle mode other than
free). For each distinct `(provider, upstream model)` in the plan that
has pricing, GPROXY counts the request's input tokens with the tokenizer
ladder and prices them at the requested tier; the highest candidate cost,
rounded up to micro-units, is the estimate. Output, cache and dimensional
metrics are not estimated. The estimate is added to a pending counter
(`gproxy:quota-pending:{window}`) for every window — total, day, week,
month, 5 h, 7 d — of every quota that applies to the caller. The request
is rejected with 402 when a window is already exhausted before the
estimate, or would be exceeded by it; the charges are then rolled back.

At settlement the actual cost is written once per `(request, window)` into
`quota_settlements` and `quota_windows.cost_used`, and the pending
estimate is released in the same atomic cache operation. A failed request
releases its estimate without recording cost. Costs travel as decimals;
only the pending counters use integer micro-units.

## Estimation Without Upstream Usage

Token counting (`gproxy-tokenize`) harvests the text of a request body,
adds four tokens per message, and tries in order: the tiktoken encoding
for GPT-family models; the Hugging Face vocabulary selected by the
provider's `tokenizer_map`, else the default vocabulary, else one named
after the model (a missing vocabulary schedules a download when enabled
and falls through); the bundled fallback vocabulary; and finally the
character estimate `ceil(characters / 2)`. Admission and credential TPM
checks use this ladder.

When a billable response carries no usage at all, settlement estimates
`input_tokens = ceil(request body characters / 2)` and
`output_tokens = ceil(response characters / 2)` (streamed bytes are
counted as they pass), records `usage_source = estimated`, and prices
that. A `web_search` response without usage still bills one
`web_searches`.
