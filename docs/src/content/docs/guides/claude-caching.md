---
title: Prompt Caching
description: "How GPROXY v4 places prompt cache breakpoints: the three magic strings, which channels honour them, where each dialect puts a marker, and how cached tokens are priced."
---

A prompt cache matches an exact prefix, so caching pays off when long, stable
instructions come before the text that changes on every turn. Placing the
breakpoint is the client's job — except that many clients cannot do it, because
their request shape has no field for one.

For those, GPROXY reads a **magic string** embedded in prompt text. That is the
whole mechanism in v4: there is no `cache_breakpoint` rule kind and there are
no cache presets. Both existed in v3 and neither was ported.

## The Three Strings

```text
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY
```

| String | On a Claude target | On an OpenAI target |
| --- | --- | --- |
| …`7D9ASD…` | `cache_control` with the API's default TTL | an explicit breakpoint |
| …`49VA1S…` | `cache_control` with `"ttl": "5m"` | an explicit breakpoint |
| …`1FAS5G…` | `cache_control` with `"ttl": "1h"` | an explicit breakpoint |

All three produce the same OpenAI breakpoint, because OpenAI has no per-block
TTL.

They are **frozen**: they are part of the client-to-proxy protocol and do not
change between releases.

## The Rule That Matters Most

**A token is always stripped, whether or not caching is enabled.** A leaked
marker sitting in somebody's prompt is never wanted.

What the switch decides is only whether a *breakpoint* is placed where the
token was. With magic cache off, the tokens come out and nothing else is
touched: the client's own `cache_control` or `prompt_cache_breakpoint` survives
untouched, and a body with no token is forwarded byte for byte, never parsed
and re-serialized.

The bare strip pass is also what `claudeweb` applies, since a browser session
has no cache control at all.

## Enabling It

Two provider `config` switches, one per family:

```json
{ "config": { "enable_claude_magic_cache": true,
              "enable_openai_magic_cache": true } }
```

| Switch | Channels that offer it |
| --- | --- |
| `enable_claude_magic_cache` | `aws_bedrock`, `azure`, `claudeapi`, `claudecode`, `custom`, `opencodego`, `opencodezen`, `openrouter` |
| `enable_openai_magic_cache` | `aws_bedrock`, `azure`, `openai`, `codex`, `custom`, `opencodego`, `opencodezen`, `openrouter` |

The switch is read against the **target dialect**, not the client's. A request
from an OpenAI-shaped client routed to a Claude provider is converted to Claude
Messages first, so the Claude switch is the one that applies and the marker
written is `cache_control`. The client keeps receiving its own format.

## Where a Marker Lands

### Claude Messages

The body is canonicalized first: string content becomes block arrays, empty
text blocks are dropped, and a `cache_control` sitting on a dropped block moves
to the nearest cacheable block — which is what makes a lone token on its own
line work. Thinking blocks are kept only on assistant turns.

Then `system` is walked **before** `messages`, so the budget is spent in prompt
order rather than in JSON key order. Every text block carrying a token is
marked:

```json
{ "cache_control": { "type": "ephemeral", "ttl": "1h" } }
```

A block that already has `cache_control` is left alone.

### OpenAI Chat Completions

Every message except the `function` role can be marked. A plain-string
`content` is split into a marked `text` part, because a marker needs somewhere
to sit; an array content has its parts marked in place.

```json
{ "type": "text", "text": "…", "prompt_cache_breakpoint": { "mode": "explicit" } }
```

### OpenAI Responses

Three places, in order:

1. **`instructions`.** A token there is stripped and a marked developer message
   is prepended in front of `input` — `instructions` is a string and cannot
   carry a marker itself.
2. **`prompt.variables`**, marked like Chat parts.
3. **`input`**, whether it is a bare string (which becomes one marked user
   message) or an array of items, whose `content` and `output` are marked with
   the `input_text` / `output_text` kinds.

### Gemini

No breakpoints. Tokens are stripped and nothing else happens.

## The Cap of Four

**At most four breakpoints leave the proxy, the client's own included.** The
budget is computed by counting the markers already in the body, so a client
that placed three of its own leaves room for one.

Tokens beyond the cap are still stripped — they just do not become markers.
That is the same rule as everywhere else: the string never reaches an upstream.

## Usage and Pricing

Cached tokens are their own metric, and settlement prices them from your rate
rows:

| What an upstream reports | The metric |
| --- | --- |
| cache read | `cached_input_tokens` |
| 5-minute cache write | `cache_creation_5m_tokens` |
| 30-minute cache write | `cache_creation_30m_tokens` |
| 1-hour cache write | `cache_creation_1h_tokens` |

`cached_input_tokens` is capped at `input_tokens`: the uncached remainder is
priced at the input rate and the cached part at the cache-read rate, which
**falls back to the input rate** when no cache-read rate exists. Cache writes
with no rate row of their own settle at zero. See
[Pricing & Tiers](/reference/pricing/).

A token count is optional on a record — an upstream that did not report a field
did not measure zero — while every total is a plain number, because a sum of
"nothing reported" really is zero.

## Keeping a Hit

Rewrite rules run **after** conversion and before the channel, so a rule that
edits prompt text can turn an expected hit into a miss. Put the stable content
first, the marker after it, and everything per-request after the boundary — and
check that no rewrite rule with a broad `paths` selector is editing the prefix.

See [Rewrite Rules & Operation Overrides](/guides/rules/).
