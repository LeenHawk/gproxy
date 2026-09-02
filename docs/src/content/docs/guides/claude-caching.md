---
title: Prompt Caching
description: "Place Claude and OpenAI cache breakpoints with cache_breakpoint rules or GPROXY magic strings, and see how cache reads and writes are billed"
---

Prompt caches match an exact prefix, so caching pays off when long, stable
instructions come before the text that changes on every turn. GPROXY marks
that prefix after the request has been converted to the upstream's wire
format, so the same configuration works whether the client speaks OpenAI,
Claude or Gemini.

There are two ways to add a breakpoint:

- A `cache_breakpoint` rule in a rule set attached to the provider (see
  [Routing Rules & Rule Sets](/guides/rules/)). The operator owns the policy.
- A **magic string** embedded in prompt text by the client, for clients that
  cannot send `cache_control` themselves. The provider must have the
  matching magic-cache switch enabled.

Markers a client already sends survive conversion: a Claude `cache_control`
on a text block becomes an explicit OpenAI `prompt_cache_breakpoint`, and an
OpenAI breakpoint becomes an ephemeral `cache_control` without a TTL.

## Cache Breakpoint Rules

```json
{ "kind": "cache_breakpoint", "target": "system", "index": null, "ttl": "1h" }
```

| Field | Meaning |
| --- | --- |
| `target` | `top_level` (alias `global`), `system`, `tools`, `message`. |
| `index` | Signed, one-based position in the target's flat list of cacheable blocks. Positive counts from the start, negative from the end, `null` selects the last block, `0` is invalid. |
| `ttl` | Claude: `5m` or `1h`, written into `cache_control.ttl`; omitted means the provider default. OpenAI: `30m` sets `prompt_cache_options.ttl`; other values are ignored. |

Behaviour by target format:

| Target | Claude Messages | OpenAI Chat / Responses | Gemini |
| --- | --- | --- | --- |
| `top_level` | Adds request-level `cache_control` unless one exists. | Sets `prompt_cache_options.mode` to `implicit` unless the client chose a mode; adds `ttl: "30m"` when requested. | skipped |
| `system` | Marks the selected block of `system[]`. | Chat: a `system` or `developer` message. Responses: `instructions` (GPROXY inserts a one-space `developer` item that carries the marker, because `instructions` cannot) or a `system`/`developer` input item. | skipped |
| `tools` | Marks a tool definition. | skipped | skipped |
| `message` | Flattens the cacheable blocks of every `messages[].content` in prompt order, then applies `index`. | Chat: every message except the `function` role. Responses: the `input` string, or every input item with a role and content. | skipped |

String content is normalised to a text block (Claude) or a text part
(OpenAI) before it is marked, so a plain-string system prompt can carry a
breakpoint.

Claude never carries more than four markers: a rule that would add a fifth
is skipped, and a block that already has `cache_control` is left alone.
Blocks Claude cannot cache are never selected: `thinking`,
`redacted_thinking`, citation and location blocks, empty text, and images or
documents outside `user` messages. OpenAI parts that can be marked are
`text`, `image_url`, `input_audio`, `file` and `refusal` (Chat) and
`input_text`, `input_image`, `input_file`, `output_text` and `refusal`
(Responses).

The resulting markers:

```json
{ "cache_control": { "type": "ephemeral", "ttl": "1h" } }
```

```json
{ "type": "input_text", "text": "…", "prompt_cache_breakpoint": { "mode": "explicit" } }
```

## Magic Strings

A client that cannot set `cache_control` can put one of three fixed strings
inside a text block. GPROXY removes the string before the request leaves and
places the native marker on that block.

| String | Claude result | OpenAI result |
| --- | --- | --- |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH` | `cache_control` with the provider default TTL | explicit breakpoint |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF` | `cache_control` with `ttl: "5m"` | explicit breakpoint |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY` | `cache_control` with `ttl: "1h"` | explicit breakpoint |

The strings are frozen: they are part of the client-to-proxy protocol and
do not change between releases. Behaviour:

- The pass is opt-in per provider through two typed settings,
  `enable_claude_magic_cache` and `enable_openai_magic_cache`, shown under
  the provider's advanced settings. The Claude switch exists on the Claude
  API, Claude Code, AWS Bedrock, Azure, Custom, OpenCode, OpenRouter and
  Vercel channels; the OpenAI switch on OpenAI, Codex, Azure, Custom,
  OpenCode, OpenRouter and Vercel.
- Every occurrence of any of the three strings is stripped from every
  `text` field, even after the marker cap is reached.
- Existing markers count toward the cap of four per request. Once four are
  present, further strings are still stripped but no marker is written.
- On Claude targets the pass runs after rule sets, so a `system_text` rule
  can itself carry a magic string. GPROXY normalises the body first: string
  content becomes text blocks, empty text blocks are dropped, and a marker
  sitting on a dropped block moves to the previous cacheable block.
- On OpenAI targets the pass runs inside the channel. Chat message strings
  become a marked `text` part; a marked Responses `instructions` string is
  stripped and a one-space `developer` item carries the breakpoint;
  `prompt.variables` and `input` strings or parts are marked in place. All
  three strings produce the same explicit breakpoint because OpenAI has no
  per-block TTL.

## OpenAI Clients on Claude Providers

A request from an OpenAI-format client routed to a Claude provider is
converted to Claude Messages before any rule runs. A `cache_breakpoint` rule
on that provider therefore writes `cache_control`, and a magic string in the
client's text is honoured under the Claude switch. The client keeps
receiving responses in its own format.

## Presets

The Rules workspace ships two cache presets, both filtered to
`generate_content` and `stream_generate_content` with a `1h` TTL:
**Claude system cache** marks the last `system` block and **Claude message
cache** marks the last cacheable message block. Applying one creates an
ordinary, editable rule set attached to the provider. No channel seeds a
cache rule on its own.

## Usage and Pricing

Claude reports cache activity in `usage`; GPROXY maps it as follows.

| Claude usage field | GPROXY | Console column |
| --- | --- | --- |
| `cache_read_input_tokens` | `cached_input_tokens`; also included in `input_tokens` | Cache read |
| `cache_creation.ephemeral_5m_input_tokens` | metric `cache_creation_5m_tokens` | Cache write 5min |
| `cache_creation.ephemeral_1h_input_tokens` | metric `cache_creation_1h_tokens` | Cache write 1h |
| legacy `cache_creation_input_tokens` | metric `cache_creation_5m_tokens` | Cache write 5min |

Settlement prices cached tokens at the rule's `cache_read_price` and falls
back to the input price when none is set. Cache writes are priced per
million tokens by `cache_creation_5m_price`, `cache_creation_30m_price` and
`cache_creation_1h_price` on a tier row, or by a dimensional `price_rates`
row for the matching metric; with neither, cache writes settle at zero.
Cache-creation tokens count toward `min_prompt_tokens` thresholds when a
context ladder selects the tier. See [Pricing & Tiers](/reference/pricing/).

## Rule Order and Cache Hits

Request rules run `system_text -> cache_breakpoint -> rewrite -> transform
-> header`, so a later `rewrite` or `transform` can still change text before
a breakpoint and turn an expected hit into a miss. Keep stable content
first, place the breakpoint after it, and keep per-request text after the
boundary.
