---
title: Prompt Caching
description: Add Claude or OpenAI cache breakpoints with provider rules or GPROXY magic strings.
---

Prompt caching is most effective when long, repeated instructions and reference
material come before the content that changes on every request. GPROXY can mark
that stable prefix after it has converted the request into the upstream
protocol, so the same rule works even when the client and upstream use different
API formats.

There are two ways to add a breakpoint:

- Use a `cache_breakpoint` rule when the provider owns the cache policy.
- Enable **Magic-string cache** when the client needs to choose the boundary
  inside prompt text.

## Manual Breakpoint Rules

Create a rule with `kind: "cache_breakpoint"`. For example, this marks the
system prefix of an OpenAI request and uses OpenAI's request-wide 30-minute TTL:

```json
{
  "target": "system",
  "ttl": "30m"
}
```

The rule accepts these fields:

| Field | Meaning |
| --- | --- |
| `target` | `top_level`, `system`, `tools`, or `message`. Protocol support differs; see the table below. |
| `index` | Signed, one-based block index. Positive values count from the start, negative values count from the end, and an omitted value selects the last block. `0` is invalid. |
| `ttl` | Claude: `5m` or `1h`. OpenAI: `30m`. |
| `position` | Kept for compatibility and currently ignored. |

Rules run after protocol conversion. If an OpenAI client is routed to Claude,
the rule sees a Claude Messages body and writes a Claude marker. If the target
is OpenAI Chat or Responses, it writes an OpenAI marker.

### Target behavior

| Target | Claude Messages | OpenAI Chat / Responses |
| --- | --- | --- |
| `top_level` | Adds request-level `cache_control`, enabling Anthropic's automatic prompt caching. | Ensures `prompt_cache_options.mode` is `implicit` unless the request already chose a mode. |
| `system` | Marks a block in the `system` array. A string-form `system` cannot carry block metadata and is skipped. | Marks system/developer content. For Responses `instructions`, GPROXY inserts a small developer content block immediately after the instructions because `instructions` itself cannot carry breakpoint metadata. |
| `tools` | Marks a tool definition. | Unsupported by OpenAI and skipped with a warning. |
| `message` | Marks a content block in the final element of the transformed `messages` array, regardless of role. | Marks a supported content block in the final Chat message element or the final Responses input item containing a role and content. |

## OpenAI Breakpoints

OpenAI Chat Completions, Responses, and Responses WebSocket use this marker on a
supported content part:

```json
{
  "type": "input_text",
  "text": "Stable reference material",
  "prompt_cache_breakpoint": {
    "mode": "explicit"
  }
}
```

For Chat Completions, the equivalent text-part type is `text`. The OpenAI TTL is
request-wide, so a rule with `"ttl": "30m"` also adds:

```json
{
  "prompt_cache_options": {
    "ttl": "30m"
  }
}
```

Adding a breakpoint does not replace a client-supplied
`prompt_cache_options.mode`. Leave the mode at its default `implicit` to combine
OpenAI's automatic breakpoint with explicit ones, or set it to `explicit` when
only your marked boundaries should be used.

Explicit breakpoints require GPT-5.6 or a later model family. Older models can
reject `prompt_cache_options` and `prompt_cache_breakpoint`. For more reliable
matching, send a stable `prompt_cache_key` for requests that share the same
prefix. OpenAI only caches prefixes that meet its minimum token threshold, so a
breakpoint on a short prompt will not produce a cache hit.

## Claude Breakpoints

Claude uses `cache_control` on the selected block:

```json
{
  "cache_control": {
    "type": "ephemeral",
    "ttl": "5m"
  }
}
```

For a one-hour Claude TTL, attach a `header` rule to the same provider:

```json
{
  "name": "anthropic-beta",
  "value": "extended-cache-ttl-2025-04-11",
  "mode": "merge"
}
```

Use `merge` so any beta features already requested by the client are preserved.

## Magic Strings

Turn on **Magic-string cache** in provider settings to let a client place a
trigger directly in a text block. This setting is available for OpenAI, Codex,
Claude API, Claude Code, OpenRouter, and Vercel channels. GPROXY removes the
trigger before sending the request and adds the native cache marker at that
location.

The trigger strings are shared across Claude and OpenAI formats:

| Trigger | Claude result | OpenAI result |
| --- | --- | --- |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH` | `cache_control` with the provider default TTL | Explicit breakpoint |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF` | `cache_control` with `5m` | Explicit breakpoint |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY` | `cache_control` with `1h` | Explicit breakpoint |

OpenAI currently supports only the request-wide `30m` TTL, so all three strings
produce the same OpenAI marker. Existing markers count toward the four-marker
limit. Extra trigger strings are still removed, but no additional marker is
written after the limit is reached.

## Rule Order and Cache Hits

Request rules run in this order:

```text
system_text -> cache_breakpoint -> rewrite -> transform -> header
```

Later rewrite or transform rules can still change text before a breakpoint.
That changes the cached prefix and can turn an expected hit into a miss. Keep
stable content first, place the breakpoint after it, and keep request-specific
text after the boundary.
