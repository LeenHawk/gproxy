---
title: Credential Selection and Cache Affinity
description: Pick modes, internal cache-affinity pool design, hit judgment, and OpenAI/Claude/Gemini cache-hit practices.
---

## Why this page exists

When a provider has multiple credentials, cache hit rate and credential selection are coupled:

- If cache-sensitive requests keep switching credentials, upstream cache hit rate usually drops.
- If everything is pinned to one credential, throughput and failover degrade.

GPROXY balances this with pick modes plus an internal in-memory cache-affinity pool.

## Pick mode configuration

Configure these fields in `channels.settings`:

- `credential_round_robin_enabled` (default `true`)
- `credential_cache_affinity_enabled` (default `true`)
- `credential_cache_affinity_max_keys` (default `4096`)

Effective modes:

| `credential_round_robin_enabled` | `credential_cache_affinity_enabled` | Effective mode | Behavior |
|---|---|---|---|
| `false` | `false/true` | `StickyNoCache` | no round-robin, no affinity pool, always pick the smallest available credential id |
| `true` | `true` | `RoundRobinWithCache` | round-robin among eligible credentials with affinity matching |
| `true` | `false` | `RoundRobinNoCache` | round-robin among eligible credentials, no affinity matching |

Notes:

- `StickyWithCache` is intentionally not supported.
- If round-robin is disabled, affinity is forced off.
- Legacy field `credential_pick_mode` is still parsed for compatibility.

## Internal affinity pool design

GPROXY keeps a process-local map:

- key: `"{channel}::{affinity_key}"`
- value: `{ credential_id, expires_at }`
- store: `DashMap<String, CacheAffinityRecord>`
- each channel retains at most `credential_cache_affinity_max_keys` keys; before inserting a new key, expired keys are pruned first, then the earliest-expiring keys are evicted if the limit is still exceeded

The pool is process-local and not persisted. It resets on restart.

## Hit judgment and retry behavior

`RoundRobinWithCache` uses a multi-candidate hint:

- `CacheAffinityHint { candidates, bind }`
- each candidate has `{ key, ttl_ms, key_len }`

Selection flow:

1. Build candidate keys from request body with protocol-specific block/prefix rules.
2. Scan candidates in order. The scan stops at the first miss, or at the first hit whose mapped credential is not currently eligible.
3. For the contiguous hit prefix, sum `key_len` by credential and pick the credential with the largest total score.
4. If scores tie, pick the credential that appears earlier in the current eligible list.
5. If no candidate is usable, fall back to normal round-robin among eligible credentials.
6. On success, always bind the `bind` key and refresh the matched key (if any).
7. If an affinity-picked attempt fails and retries, only clear the matched key for that attempt.

Important:

- This is GPROXY internal routing affinity, not the upstream provider's native cache-hit decision.

## Key derivation and TTL rules by protocol

GPROXY no longer uses whole-body hash for these content-generation requests. It uses canonicalized block prefixes.

Shared rules:

- Canonical JSON per block: sorted object keys, `null` removed, arrays keep order.
- Rolling prefix hash: `prefix_i = sha256(seed + block_1 + ... + block_i)`.
- Non-Claude candidate sampling:
  - `<=64` boundaries: all
  - `>64`: first 8 and last 56
  - match priority: longest prefix first
- `stream` does not participate in key derivation.

### OpenAI Chat Completions

Block order:

- `tools[]`
- `response_format.json_schema`
- `messages[]` (split by content blocks)

Key format:

- `openai.chat:ret={ret}:k={prompt_cache_key_hash|none}:h={prefix_hash}`

TTL:

- `prompt_cache_retention == "24h"` -> 24h
- otherwise -> 5m

### OpenAI Responses

Block order:

- `tools[]`
- `prompt(id/version/variables)`
- `instructions`
- `input` (split item/content blocks)

Key format:

- `openai.responses:ret={ret}:k={prompt_cache_key_hash|none}:h={prefix_hash}`

TTL:

- `prompt_cache_retention == "24h"` -> 24h
- otherwise -> 5m

Not included in prefix key:

- `reasoning`
- `max_output_tokens`
- `stream`

### Claude Messages

Block hierarchy:

- `tools[] -> system -> messages.content[]`
- Claude shorthands are normalized before splitting: `system: "..."` and `messages[*].content: "..."` each become one text block

Breakpoints:

- explicit: block has `cache_control`
- automatic: top-level `cache_control` exists, then use the last cacheable block (fallback backward if needed)

Candidates:

- for each breakpoint, include up to 20 lookback boundaries
- merge and dedupe candidates
- priority: later breakpoint first, then longer prefix first

Key format:

- `claude.messages:ttl={5m|1h}:bp={explicit|auto}:h={prefix_hash}`

TTL:

- explicit or automatic breakpoint `ttl == "1h"` -> 1h
- explicit or automatic breakpoint `ttl == "5m"` -> 5m
- when `cache_control` is present but `ttl` is omitted (`{"type":"ephemeral"}`), built-in Claude defaults use 5m

If request has no explicit breakpoint and no top-level `cache_control`, affinity hint is not generated.

Important ordering constraint:

- Anthropic validates breakpoint TTL order in processing hierarchy (`tools -> system -> messages`).
- a `ttl="1h"` breakpoint must not appear after a `ttl="5m"` breakpoint in that order.
- if you mix 1h and 5m, place all 1h breakpoints earlier in the hierarchy.

### Gemini GenerateContent / StreamGenerateContent

If `cachedContent` exists:

- key: `gemini.cachedContent:{sha256(cachedContent)}`
- TTL: 60m

Otherwise prefix mode:

- block order: `systemInstruction -> tools[] -> toolConfig -> contents[].parts[]`
- key: `gemini.generateContent:prefix:{prefix_hash}`
- TTL: 5m

Not included by default:

- `generationConfig`
- `safetySettings`

## Claude / ClaudeCode cache rewrite and magic triggers

`enable_top_level_cache_control` is deprecated. Use `cache_breakpoints` instead.

Rewrite sources for `claude` / `claudecode`:

- provider-level `channels.settings.cache_breakpoints`
- request payload existing `cache_control` (kept as-is)
- magic trigger strings in `system[].text` and `messages[].content[].text`

Magic trigger behavior:

- gproxy removes the trigger token from text before forwarding upstream
- if target block does not already have `cache_control`, gproxy injects one
- if block already has `cache_control`, only token removal is applied
- injected breakpoints from magic triggers plus existing request breakpoints are capped at 4 total
- when the 4-breakpoint budget is exhausted, gproxy still removes trigger tokens but skips new `cache_control` injection

Supported trigger tokens:

- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH`
  - inject `{"type":"ephemeral"}`
- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF`
  - inject `{"type":"ephemeral","ttl":"5m"}`
- `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY`
  - inject `{"type":"ephemeral","ttl":"1h"}`

## Upstream cache mechanisms (provider-side)

These are provider behaviors, independent from GPROXY affinity internals.

### OpenAI

- Prompt caching is exact-prefix based.
- Requests are routed by a prefix hash; `prompt_cache_key` is combined with that routing key.
- Keep static content (instructions/tools/examples/images schema) in the prefix, and move variable content to the tail.
- Retention (`in_memory` vs `24h`) affects lifetime behavior, but cache matching remains prefix-oriented.

### Claude

- Prefix hierarchy is `tools -> system -> messages`.
- Supports explicit block-level breakpoints and automatic top-level `cache_control`.
- Uses backward sequential checking with a 20-block lookback window around breakpoints.
- Cacheability depends on block eligibility; ordering and breakpoint placement directly impact hit rate.

### Gemini

- Explicit context caching is centered around `cachedContent` reuse (cached content is treated as prompt prefix).
- Implicit caching is provider-managed and works best when similar prefixes are sent close in time.
- Reusing the same `cachedContent` handle typically improves explicit-cache hit behavior.
- GPROXY currently supports generation routes and does not expose cached-content management routes.

## Practical tips

1. Keep prefix content byte-stable (model, tools, system, long context ordering).
2. Use `RoundRobinWithCache` for cache-sensitive traffic.
3. Avoid unnecessary credential churn inside short cache windows.
4. Split very different prompt workloads into different channels/providers.
5. Prefer explicit `cache_breakpoints` TTL (`5m` / `1h`) when you need deterministic Claude/ClaudeCode behavior.
6. Prefer explicit `cachedContent` reuse in Gemini workflows when available.

## Usage examples

Round-robin + cache affinity:

```toml
[channels.settings]
credential_round_robin_enabled = true
credential_cache_affinity_enabled = true
```

Round-robin without affinity:

```toml
[channels.settings]
credential_round_robin_enabled = true
credential_cache_affinity_enabled = false
```

No round-robin (sticky smallest-id available credential):

```toml
[channels.settings]
credential_round_robin_enabled = false
```
