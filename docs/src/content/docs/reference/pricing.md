---
title: "Pricing & Tiers"
description: "How GPROXY v4 prices one exchange: rule selection, rate rows and their conditions, the context and service-tier ladders, and what happens with no price at all."
---

Pricing answers one question at settlement: **what did this exchange cost**,
given the provider, the upstream model and the normalized usage the channel
extracted.

It happens **inside the engine**, at settlement, so budgets and the observer
see the same number. In v3 the engine handed usage up and an application layer
priced it again, which produced two time semantics, two model matchers and two
answers to "what if there is no price". v4 has one of each.

Three tables, in one shape: a `price_rules` row selects a model, its
`price_rates` rows price each metric, and its `price_tiers` rows adjust the
token ladder by prompt size and service tier.

## Rule Selection

A rule fits when

- its provider is the exchange's provider, **or is unset** (a global rule);
- its `modelPattern` glob matches the **upstream** model name;
- its `operation` is the request's, **or is unset** (all operations).

**Provider rules precede global rules.** Within one scope, the lowest
`(priority, id)` wins.

| Field | Meaning |
| --- | --- |
| `providerId` | `null` is a global rule |
| `modelPattern` | `*` / `?` glob against the upstream model name |
| `operation` | `null` covers every operation of the matched model |
| `priority` | lower first; the id breaks ties |
| `currency` | e.g. `USD`. Every price under this rule is in it |
| `enabled` | |

### No rule at all

The exchange is **unpriced**: it costs nothing, it still settles, and its usage
carries

```json
{"dimensions": {"unpriced": "true"}}
```

That is a signal, not a refusal. An operator wants to know that a model is
being served for free; they do not want a request rejected because nobody has
entered a price yet.

## Rate Rows

A rate row is the base price of one metric.

| Field | Meaning |
| --- | --- |
| `metric` | a built-in key, or any custom one a channel emits |
| `unit` | `token`, `count`, `second` or `character` |
| `unitQuantity` | the positive denominator: 1,000,000 tokens, 1 image, 60 seconds |
| `value` | the non-negative price for `unitQuantity` units, in the rule's currency |
| `conditions` | `null` is the fallback row; otherwise a non-empty object of dimension names to scalars, **all** of which must match |
| `priority` | lower first among rows of the same metric |

For one metric, conditional rows are tried in `(priority, id)` order and the
first whose conditions all match the settlement's dimensions wins; otherwise
the fallback row applies. **A selected conditional rate replaces the base
rate**, it does not compose with it.

```json
[{"metric":"input_tokens",  "unit":"token","unitQuantity":"1000000","value":"0.40"},
 {"metric":"output_tokens", "unit":"token","unitQuantity":"1000000","value":"1.60"},
 {"metric":"image_outputs", "unit":"count","unitQuantity":"1","value":"0.04",
  "conditions":{"quality":"hd","size":"1024x1024"},"priority":0}]
```

### The rule that stops double billing

**Rates must consume disjoint quantities.** Cached input is removed from
ordinary input; a reasoning or media token subset must not be charged again
when it is already billed through an aggregate. Per-image and per-token output
are *alternative* bases, unless the provider really charges both.

A generic `tool_calls` row and a specific `web_searches` row must not both bill
the same call. And a client *declaring* a tool is not a billable server-side
execution.

## The Metrics

**Tokens** — conventionally per 1,000,000:

```text
input_tokens          output_tokens           cached_input_tokens
cache_creation_5m_tokens  cache_creation_30m_tokens  cache_creation_1h_tokens
reasoning_tokens      image_input_tokens      image_output_tokens
audio_input_tokens    cached_audio_input_tokens  audio_output_tokens
video_input_tokens    video_tokens
```

**Counts, seconds and characters:**

```text
image_outputs   video_outputs   audio_seconds   video_seconds   audio_characters
search_units    web_searches    web_fetches     file_searches
code_interpreter_sessions       tool_calls      requests
```

A metric outside this list is still priced as long as a rate row names it. The
list is what the built-in keys are, not a filter.

`requests` is the per-request fee: one request is charged per **priced
exchange**.

## Tiers

A `price_tiers` row overrides **token prices only**, always per million.

| Field | Meaning |
| --- | --- |
| `serviceTier` | `null` makes it a **context** tier; otherwise `standard`, `priority`, `flex`, `batch`, … |
| `minPromptTokens` | a non-negative inclusive threshold; default 0 |
| `multiplier` | for a service tier: what to multiply the inherited price by. Context-only rows leave it unset |
| `*_per_million` | an explicit price for one token kind |

The explicit columns are `input`, `output`, `cache_read`,
`cache_creation_5m` / `30m` / `1h`, `reasoning`, `image_input`,
`image_output`, `audio_*` and `video_*`.

**Prompt length counts input plus cache reads and cache writes, once.**

### Composition, per token kind

1. **The base rate** — the `price_rates` row for that metric, conditional rows
   first.
2. **The context tier** — the highest reached `minPromptTokens` among rows with
   **no** service tier. Its explicit prices replace the base for the kinds it
   sets.
3. **The service tier** — the highest reached threshold among rows naming the
   *actual* service tier. An explicit price there **wins outright**; otherwise
   the context-adjusted base is multiplied by that row's `multiplier`
   (default 1).

Equal thresholds break on the lower `(priority, id)`. `null` inherits; zero is
**explicitly free**, not missing.

A multiplier never applies to tool or media *counts* — only to token prices.

:::caution[An explicit tier price replaces the whole ladder]
With a base input price of 1, a context step `≥ 200,000 → 2`, and 300,000
prompt tokens:

| The `batch` row | Effective input rate |
| --- | --- |
| `{"serviceTier":"batch","multiplier":"0.5"}` | `2 × 0.5 = 1` |
| `{"serviceTier":"batch","inputPerMillion":"0.5"}` | `0.5` — the 200k step is lost |
| `{"serviceTier":"batch","minPromptTokens":200000,"inputPerMillion":"1"}` | `1` |

Repeat an explicit tier price at every threshold it must cover, or use a
multiplier.
:::

### Two special cases

- **Reasoning is a subset of output.** When a reasoning price exists, the
  reasoning tokens are subtracted from `output_tokens` and charged separately.
  When it does not, they stay inside output and are charged once.
- **A cache read with no price inherits the input price.** Every *other* kind
  with no price is free. That asymmetry is deliberate: a cache read is
  unambiguously an input token, while a missing audio or video price is a
  configuration gap the operator should see as zero.

### Requested versus served

The tier that is charged is the one the response **actually reported**, not the
one the request asked for. A `priority` request the provider downgrades settles
at the downgraded row.

## Everything Else

Every metric that is not a token kind is charged as

```text
amount × value / unitQuantity
```

and is **never multiplied by a service tier**.

## Editing Prices

```sh
curl -s http://127.0.0.1:8787/admin/api/price-rules  -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:8787/admin/api/price-rates  -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:8787/admin/api/price-tiers  -H "Authorization: Bearer $GPROXY_KEY"
```

Each family takes the usual five routes plus a batch, and a batch is one
revision commit however many rows it names.

The bundled catalogue fills them in for the models this release knew about:

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/default-model-catalog/apply-prices \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","modelIds":["gpt-4o-mini"],"overwrite":false}'
```

Without a provider, the catalogue's own glob and priority are used, so one rule
prices that model wherever it is served; with a provider, the literal name
becomes the pattern at priority zero. **`overwrite: false` is what makes
re-applying safe**: a rule an operator edited keeps its edit and is reported as
skipped.

A bad pricing row is dropped with a warning at assembly and does **not** block
the snapshot. One mistyped decimal must not take down an instance that is
otherwise serving traffic.

## Budgets Spend the Result

A budget is a `quotas` row whose metric is `cost` in USD. Admission hands the
engine the caller's chain — `[api_key?, user, subscription?, team?, org?]` —
and **every** enabled budget of **any** owner in the chain applies.

There is no pre-charge and no estimate. Cost is known when the exchange ends,
so a budget can be overrun by **at most one request**, and that is the accepted
price of never having to roll one back. See
[Usage, Logs & Audit](/guides/observability/#quotas-and-budgets).

## Token Estimation

When a billable response carries no usage at all, the engine counts the tokens
itself and marks the record `estimated = true`. Counting uses a local
vocabulary: the tiktoken encodings for GPT-family models, otherwise the
vocabulary the model's catalogue row names, otherwise the bundled DeepSeek one.

```sh
curl -s http://127.0.0.1:8787/admin/api/tokenizer-vocabs -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/tokenizer-vocabs \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"repo":"…","filename":"tokenizer.json","modelId":"…","setAsDefault":true}'
```

A fetch downloads the file through the configured file storage, then inserts
the row and points the model and the default at it — **the row and both
pointers in one revision commit**. The bytes land before the row: a row
pointing at an object that was never written would fail every reload, while an
object with no row is only wasted space.

Without file storage the whole family answers "unsupported": there is nowhere
to put the bytes.
