---
title: Claude Prompt Caching
description: How the anthropic and claudecode channels interact with Anthropic prompt caching, including cache breakpoint rules and magic-string triggers.
---

Anthropic's **prompt caching** lets you reuse the KV cache for the static
parts of a request (tool list, system prompt, long context) across calls
— paying a discounted "cache read" token rate instead of re-processing
the prefix every time. See Anthropic's guide:
[Prompt caching — platform.claude.com](https://platform.claude.com/docs/en/build-with-claude/prompt-caching).

The high-level rules from Anthropic that GPROXY plays inside of:

- A request can mark up to **4 cache breakpoints** using
  `cache_control: { type: "ephemeral" }` on a content block.
- Breakpoints are **positional**: everything *before* the breakpoint in
  the canonical request order is what gets cached, keyed on content
  equality.
- Cache entries are **ephemeral** with two TTLs — **5 minutes** (default)
  or **1 hour** (requires the `extended-cache-ttl-2025-04-11` beta
  header).
- The cache is scoped to the model + beta header set + tool definitions
  + system content, so anything that changes earlier in the request
  invalidates everything after it.

GPROXY's `anthropic` and `claudecode` channels both add two
server-side ways to inject those breakpoints so that clients which don't
know about caching can still benefit:

1. **Cache breakpoint rules** (`settings.cache_breakpoints`) — a list of
   positional rules applied on the server.
2. **Magic-string triggers** (`settings.enable_magic_cache`) — the
   client embeds a sentinel token in its text, and the server rewrites
   it into a `cache_control` block.

Both features touch the request body inside `finalize_request(...)`
(after protocol transform, before credential selection), so they apply
uniformly regardless of whether the client originally spoke Claude,
OpenAI, or Gemini — as long as the resolved upstream is an Anthropic
channel.

Implementation: [`sdk/gproxy-channel/src/utils/claude_cache_control.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-channel/src/utils/claude_cache_control.rs).

## Cache breakpoint rules

A rule declares *where* to place a `cache_control` marker and *which*
TTL to use. Each provider entry can carry up to **4 rules** — matching
Anthropic's 4-breakpoint limit.

```jsonc
{
  "cache_breakpoints": [
    { "target": "tools",    "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "system",   "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "messages", "position": "last_nth", "index": 1, "ttl": "5m"   },
    { "target": "messages", "position": "last_nth", "index": 3, "ttl": "5m"   }
  ]
}
```

### Fields

| Field | Values | Meaning |
| --- | --- | --- |
| `target` | `top_level` (alias `global`) \| `tools` \| `system` \| `messages` | Which section of the Claude request body to place the breakpoint in. |
| `position` | `nth` (default) \| `last_nth` / `last` / `from_end` | Count from the start or the end of that section. |
| `index` | positive integer, default `1` | 1-based index. `1` + `last_nth` = the last block. |
| `ttl` | `auto` (default) \| `5m` \| `1h` | Cache TTL. `auto` leaves the ttl off and uses Anthropic's default (5 m). `1h` requires the `extended-cache-ttl-2025-04-11` beta (see below). |

### Targets

- **`tools`** — places the breakpoint inside the `tools` array. Useful
  when your tool list is large and stable across turns.
- **`system`** — places the breakpoint inside the `system` content
  array. Useful for long system prompts.
- **`messages`** — places the breakpoint on a specific content block of
  the `messages` array. Useful for long pinned context at the start of
  the conversation.
- **`top_level`** / **`global`** — a target for future top-level caching
  hooks; rules with this target are parsed and kept but currently place
  the marker inside the request top level.

### Positional semantics

`position: "nth"` counts from the front; `position: "last_nth"` counts
from the back. Both use 1-based indexing:

- `{ target: "messages", position: "nth", index: 1 }` — first message.
- `{ target: "messages", position: "last_nth", index: 1 }` — last message.
- `{ target: "messages", position: "last_nth", index: 3 }` — third from last.

If the index is out of range for the section, that rule is silently
dropped — GPROXY never inserts a dangling `cache_control`.

### How it composes with client-supplied breakpoints

If a client has already set `cache_control` on some blocks, GPROXY
counts them and **only fills the remaining slots** (up to 4 total).
Nothing the client set is overwritten. The canonicalization step
also normalizes string-valued `system` and `content` fields into the
equivalent array-of-blocks shape so the rules have something concrete
to attach to.

## Magic-string triggers

Sometimes you can't configure the server and have to embed caching
intent inside the prompt text itself. `settings.enable_magic_cache = true`
opts into that mode: the client writes a well-known sentinel string at
the end of a text block, and GPROXY rewrites that block to carry
`cache_control: { type: "ephemeral", ttl: "..." }` and removes the
sentinel.

Three sentinels are recognized (constants defined in
`claude_cache_control.rs`):

| Sentinel | TTL written |
| --- | --- |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH` | `auto` (default) |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF` | `5m` |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY` | `1h` |

Same 4-breakpoint cap applies — the server counts how many
`cache_control` markers already exist on the request and only fills
remaining slots.

The intended use is for tools that can render a dynamic prompt but
can't configure the server: a client library that knows the sentinel
can inject it programmatically, GPROXY strips it out before forwarding,
and upstream sees a normal request with a `cache_control` block in
exactly the right place.

## `auto` vs. explicit TTL

- **`auto`** — GPROXY writes `{ "type": "ephemeral" }` with no TTL
  field. Anthropic defaults to the **5-minute** cache in this case.
  This is the safest choice and does not require any beta header.
- **`5m`** / **`1h`** — GPROXY writes the explicit TTL. For `1h`,
  Anthropic requires the `extended-cache-ttl-2025-04-11` beta header
  on the request. If you use any `ttl: "1h"` rule, make sure that beta
  is on — either by having the client set it, or by listing it in
  `settings.extra_beta_headers` so GPROXY merges it into every
  request automatically.

## Practical recipes

### Cache a long system prompt and the tool list

```jsonc
{
  "cache_breakpoints": [
    { "target": "tools",  "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "system", "position": "last_nth", "index": 1, "ttl": "auto" }
  ]
}
```

Use this when every request carries the same tool definitions and
the same system prompt — which is typical for agent loops.

### Cache long pinned context at the top of the conversation

```jsonc
{
  "cache_breakpoints": [
    { "target": "messages", "position": "nth", "index": 1, "ttl": "5m" }
  ]
}
```

The first user message (often a big pasted document or a pinned
context window) becomes the cache prefix. Subsequent turns only pay
the cache-read rate for it.

### Cache both the "warm prefix" and the "last-but-one" message

```jsonc
{
  "cache_breakpoints": [
    { "target": "tools",    "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "system",   "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "messages", "position": "nth",      "index": 1, "ttl": "5m"   },
    { "target": "messages", "position": "last_nth", "index": 2, "ttl": "5m"   }
  ]
}
```

This uses all four breakpoints. Tool list + system prompt are the
"warm prefix" across every call; the first message is long pinned
context; the second-to-last message is a recent stable turn you
expect to replay in tight loops.

### Opt into 1-hour TTL without the client knowing

```jsonc
{
  "cache_breakpoints": [
    { "target": "system", "position": "last_nth", "index": 1, "ttl": "1h" }
  ],
  "extra_beta_headers": [
    "extended-cache-ttl-2025-04-11"
  ]
}
```

`extra_beta_headers` is merged into every upstream request, so the
client doesn't need to know about the beta — GPROXY adds it before
sending.

## What to watch out for

- **4 breakpoint limit is hard.** Client-supplied `cache_control`
  blocks count against the 4, so if your client already marks
  breakpoints, your rules may have fewer slots than they appear.
- **Cache key sensitivity.** Anthropic keys the cache on the exact
  content of the prefix. If you also apply a
  [rewrite rule](/guides/rewrite-rules/) that mutates text inside the
  cached portion, you'll evict the cache every call. Put rewrites on
  fields the cache prefix doesn't touch, or place breakpoints *after*
  the rewritten content.
- **`claudecode` vs. `anthropic`.** The two channels implement the
  same cache feature set; `claudecode` additionally supports a
  `prelude_text` setting that injects an organization-wide system
  block. If you use both a prelude and a `system` breakpoint, the
  breakpoint still lands at the correct position after the prelude is
  inserted — it's evaluated on the final canonical body.
- **Minimum cacheable size.** Anthropic enforces a minimum prefix
  length for caching; very short prompts won't actually be cached
  even if a breakpoint is present. Check Anthropic's
  [prompt caching docs](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
  for the current threshold.
