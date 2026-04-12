---
title: Message Rewrite Rules
description: Regex-based text substitution applied to the text fields of a request — system prompts, user messages, Gemini parts, OpenAI instructions.
---

"Message rewrite rules" (internally `sanitize_rules`) let you rewrite the
**text content** of a request — system prompts, user/assistant messages,
Gemini `parts[*].text`, OpenAI Responses `instructions` / `input` — using
**regex pattern → replacement** pairs. They are protocol-aware: gproxy
walks the right fields for whichever wire dialect the upstream is about
to see.

This is a **different feature** from
[Request Rewrite Rules](/guides/rewrite-rules/), which manipulates
arbitrary JSON fields (temperature, stream_options, metadata, …) on the
request body. The two can coexist on the same provider; they touch
different things.

| | Request Rewrite Rules | Message Rewrite Rules |
| --- | --- | --- |
| Setting | `rewrite_rules` | `sanitize_rules` |
| Shape | `{ path, action: set \| remove, filter }` | `{ pattern, replacement }` |
| Addresses | Arbitrary JSON paths in the body | Only the text of system prompts and message content |
| Match language | Dot-notation path + glob filter | **Regex** on string contents |
| Typical use | Force a field, strip a field, inject metadata | Rewrite brand names, scrub tool identifiers, replace phrases |

Implementation: [`sdk/gproxy-provider/src/utils/sanitize.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/utils/sanitize.rs).

## Rule shape

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\bPi documentation\\b", "replacement": "Harness documentation" },
    { "pattern": "\\bpi\\b",               "replacement": "the agent" },
    { "pattern": "\\bPi\\b",               "replacement": "The agent" }
  ]
}
```

- **`pattern`** — a [Rust `regex` crate](https://docs.rs/regex/latest/regex/)
  pattern. Word boundaries (`\b`) are strongly recommended so short
  patterns don't match substrings like `pipeline`, `api`, `spirit`.
- **`replacement`** — literal substitution text. It's passed straight to
  `Regex::replace_all`, so `$1`, `$2` backreferences work if you use
  capturing groups.
- Rules are applied **in declaration order**. Put **longer phrases
  first**, then shorter single-word patterns last — otherwise a short
  pattern can eat a prefix of a longer one before the longer rule gets
  a chance to fire.
- Invalid regex patterns are silently dropped at compile time (the
  channel logs them but does not fail the request).

## Which fields get rewritten

The rule set is compiled once per request and dispatched to a
protocol-specific walker. The fields touched depend on the upstream
protocol that was chosen for this request — *not* the client-side
protocol.

| Upstream protocol | Fields sanitized |
| --- | --- |
| `claude` | `system` (string or `[{type:"text", text}]` array), `messages[*].content` (string or array of text blocks) |
| `openai_chat_completions` / `openai` | `messages[*].content` |
| `openai_response` | `instructions`, `input` (string or item array — items' `content` / `output`) |
| `gemini` / `gemini_ndjson` | `systemInstruction.parts[*].text`, `contents[*].parts[*].text`, `generationConfig.contents[*].parts[*].text` |

In every case, only the **text** inside those fields is mutated. Binary
parts, image URLs, tool calls, and structured metadata are left alone —
the walker explicitly descends into text blocks, never into everything.

## Where it fits in the pipeline

Both rewrite features run inside the channel's
`finalize_request(...)` — **after** protocol translation, **before**
credential selection and HTTP transport wrapping. That order is why the
sanitize step uses the *upstream* protocol's field layout: by the time
it runs, the body has already been transformed to the shape the
upstream expects.

The full per-request order is:

```text
permission  →  request rewrite  →  alias  →  transform  →  message rewrite  →  cache breakpoints  →  upstream
```

## Worked examples

### 1. Rebrand an upstream-baked agent name

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\bPi documentation\\b", "replacement": "Harness documentation" },
    { "pattern": "\\bpi\\b",               "replacement": "the agent" },
    { "pattern": "\\bPi\\b",               "replacement": "The agent" }
  ]
}
```

Longest-first ordering matters: if the two-character `\bpi\b` ran first,
it would match the `Pi` in `Pi documentation` and leave a broken
`the agent documentation` string before the longer rule could fire.

### 2. Scrub a tool-identifier prefix

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\bclaude-code://([a-z0-9_-]+)", "replacement": "internal://$1" }
  ]
}
```

A regex capture group keeps the tool id intact while rewriting the URL
scheme. Handy when an upstream assistant has baked a vendor-specific
scheme into its system prompt and you don't want clients to see it.

### 3. Drop a stray signature line

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\n+—\\s*Sent from my Claude app", "replacement": "" }
  ]
}
```

Empty replacement deletes the match. Useful for stripping noisy
signatures that leak into usage accounting or logs.

### 4. Normalize whitespace on Gemini inputs

```jsonc
{
  "sanitize_rules": [
    { "pattern": "[\\t ]{2,}", "replacement": " " }
  ]
}
```

Gemini is picky about leading whitespace in some `parts[*].text` blocks;
collapsing runs of spaces/tabs here is cheaper than fixing the client.

## Gotchas

- **Interacts with Claude cache keys.** Anthropic keys the prompt cache
  on the *exact* text of the cached prefix. If a sanitize rule mutates
  text inside the cached region, every request is a cache miss. Either
  scope the sanitize rule to a field outside the prefix, or move the
  [cache breakpoint](/guides/claude-caching/) past the sanitized
  content.
- **Word boundaries are your friend.** `\bpi\b` is almost always what
  you want; bare `pi` will chew through `pipeline`, `api`, `spirit`,
  `typing`, etc. See the existing unit tests for edge cases
  ([`utils/sanitize.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/utils/sanitize.rs)).
- **Only text fields, not tool payloads.** Tool-use blocks, JSON
  arguments, and image URLs are never touched, even if the regex
  technically could match on a string representation of them.
- **Per-provider, not per-user.** There's no filter by user or model —
  sanitize rules apply to every request routed through the provider.
  If you need per-user text scrubbing, do it upstream of gproxy.

## Where to configure

- **Seed TOML** — each provider's `settings.sanitize_rules` is a JSON
  array on the provider entry. Same location and shape as the example
  above.
- **Embedded console** — *Providers → {your provider} → Settings*
  exposes a structured editor for the list. Edits take effect on the
  next request; no reload is needed.

Channels that currently expose `sanitize_rules` on their settings
include `anthropic`, `claudecode`, OpenAI-compatible channels, and
most other channel types (check the `ChannelSettings::sanitize_rules`
method on the channel's settings struct).
