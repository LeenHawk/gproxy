---
title: Request Rewrite Rules
description: Rewrite or strip arbitrary JSON fields in the request body before it reaches upstream — with model / operation / protocol filters.
---

GPROXY lets you rewrite or delete fields in the **request body** before
it leaves for the upstream provider. This is the escape hatch you reach
for when a client won't stop sending `temperature: 1.0` to a model that
rejects it, when you need to force `stream_options.include_usage = true`,
or when a specific provider needs a non-standard field injected.

:::note
This page is about `rewrite_rules` — **arbitrary JSON field**
manipulation. For rewriting the **text content** of system prompts and
messages with regex pattern → replacement pairs, see
[Message Rewrite Rules](/guides/message-rewrite/). The two features are
independent and can coexist on the same provider.
:::

Rewrite rules are defined per **provider** and live in the provider's
settings JSON. They are applied in the handler layer **before** alias
resolution, using the model name the client sent — so filters that
match on the alias name still work.

Implementation: [`sdk/gproxy-provider/src/utils/rewrite.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/utils/rewrite.rs).

## The rule shape

```jsonc
{
  "path":   "temperature",               // dot-notation into the JSON body
  "action": { "type": "set", "value": 0.7 },
  "filter": {                            // optional — AND logic
    "model_pattern": "gpt-4*",
    "operations":    ["generate_content", "stream_generate_content"],
    "protocols":     ["openai_chat_completions"]
  }
}
```

- **`path`** — dot-notation address into the JSON body. `temperature`,
  `stream_options.include_usage`, `metadata.tenant`, and so on. Missing
  intermediate keys are auto-created for `set`.
- **`action.type`** — either `set` (with a `value`) or `remove`.
- **`filter`** (optional) — restricts which requests the rule fires on.
  All specified dimensions must match (AND logic). An omitted dimension
  matches everything.

The rule list is applied **in order**, so a later rule can overwrite an
earlier one on the same path.

## Action semantics

### `set`

Walks the path, creating intermediate objects if they don't exist, and
writes `value` at the leaf. `value` is any JSON value — scalar, object,
or array. If an intermediate key exists but is not an object (e.g. the
client sent `"a": "string"` and you rewrite `a.b.c`), GPROXY overwrites
it with a fresh object.

```jsonc
// Force include_usage = true on every OpenAI stream.
{
  "path":   "stream_options.include_usage",
  "action": { "type": "set", "value": true },
  "filter": { "protocols": ["openai_chat_completions"] }
}
```

### `remove`

Walks to the parent and deletes the leaf key. Missing paths are a
silent no-op.

```jsonc
// Strip temperature for clients that always send 1.0.
{
  "path":   "temperature",
  "action": { "type": "remove" }
}
```

## Filter semantics

| Dimension | Meaning | Matching |
| --- | --- | --- |
| `model_pattern` | Glob against the model name the client sent. | `*` (any run of chars), `?` (exactly one char). See [the glob tests](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/utils/rewrite.rs) for exact behavior. |
| `operations` | Allowlist of `OperationFamily` values. | Rule fires only if the current operation is in the list. |
| `protocols` | Allowlist of `ProtocolKind` values. | Rule fires only if the current protocol is in the list. |

The `model_pattern` is matched against the **original** model name the
client sent — *before* alias resolution and *before* any provider prefix
stripping. That means a filter on `chat-default` matches requests that
asked for the alias `chat-default`, not requests that asked for the
underlying real model.

## Where it fits in the pipeline

The canonical order is:

```text
permission  →  rewrite  →  alias  →  execute
```

Rewrite runs **after** permission check (so denied requests don't mutate
anything) and **before** alias resolution (so filters see the alias name
the client typed).

## Worked examples

### 1. Force a sampling cap on a specific model family

```jsonc
{
  "path":   "temperature",
  "action": { "type": "set", "value": 0.7 },
  "filter": { "model_pattern": "o3*" }
}
```

### 2. Inject a required `metadata.tenant` on every call

```jsonc
{
  "path":   "metadata.tenant",
  "action": { "type": "set", "value": "acme-prod" }
}
```

Intermediate objects are auto-created, so you don't need to pre-set
`metadata` separately.

### 3. Drop a field only for Claude clients

```jsonc
{
  "path":   "thinking",
  "action": { "type": "remove" },
  "filter": { "protocols": ["claude"] }
}
```

### 4. Rewrite `stream_options` only on streaming OpenAI calls

```jsonc
{
  "path":   "stream_options",
  "action": {
    "type":  "set",
    "value": { "include_usage": true }
  },
  "filter": {
    "operations": ["stream_generate_content"],
    "protocols":  ["openai_chat_completions"]
  }
}
```

## Ordering and YAGNI

- Rules are applied **in order**. If two rules touch the same path, the
  later one wins.
- Non-object bodies (empty GETs, arrays, null) are skipped silently —
  the rules are only meaningful for JSON-object request bodies.
- Regex is **not** supported; `model_pattern` is a simple glob.
- There is no way to read an existing value and compute a new one —
  this is a set/remove system, not a scripting language. If you find
  yourself wanting conditionals on the *value*, consider handling it
  client-side or proposing a dedicated channel extension.

## Where to configure

Two places:

- **Seed TOML** — provider `settings.rewrite_rules` is a JSON array on
  the provider entry. It's easiest to assemble the rules in the console
  and export the TOML, rather than hand-writing the JSON.
- **Embedded console** — *Providers → {your provider} → Settings*
  exposes a typed editor for the rewrite rule list. Edits take effect
  on the next request; no reload needed.

Channels that currently expose `rewrite_rules` in their settings
include OpenAI-compatible, Anthropic, Claude Code, and most of the
other channel types.
