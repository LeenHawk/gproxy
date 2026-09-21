---
title: Rewrite Rules & Operation Overrides
description: "Ordered regex replacements over request and response payloads, headers and query values; how they are filtered, ordered and applied; and the per-provider operation overrides."
---

v4 has two operator-editable mechanisms in front of an upstream, and they do
different jobs.

- **Rewrite rules** change bytes: an ordered regex replacement over selected
  JSON string values, a named header value, or a named query value, in either
  direction.
- **Operation rules and operation endpoints** change *which dialect* and
  *where*: a per-provider override of the dialects a channel declares for one
  operation, and of the URL it calls.

v3's five rule kinds — `system_text`, `cache_breakpoint`, `rewrite`,
`transform` and `header` — collapsed into the one shape below. A rule is now a
pattern, a replacement, a place to apply it and a set of filters, and nothing
else.

## Where They Run

```text
client request
  → resolve the target · admit the caller
  → convert to the provider's native dialect, when it differs
  → rewrite rules, request phase
  → the channel: URL, auth, its own headers
upstream response
  → rewrite rules, response phase  (per stream unit, for a stream)
  → convert back to the client's dialect
```

Rules therefore see **the upstream's wire shape**, not the client's. An OpenAI
Chat request routed to a Claude provider is converted to Claude Messages first,
so its rules address `system`, `messages[]` and `tools[]`.

## A Rule Set

A set is a name, an optional description and an `enabled` flag. It is attached
to one or more providers; an attachment has its own `sortOrder` and `enabled`.

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/rule-sets \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"name":"demo"}'

curl -s -X POST http://127.0.0.1:7070/admin/api/provider-rule-sets \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","ruleSetId":"…","sortOrder":0}'
```

A whole set is replaced in one call, which is how a set is edited as a unit
rather than row by row:

```sh
curl -s -X PUT http://127.0.0.1:7070/admin/api/rule-sets/{id}/rules \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '[ … ]'
```

## A Rule

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/rules \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"ruleSetId":"…","phase":"request","target":"body",
       "paths":["messages.*.content"],
       "pattern":"(?i)\\bwidget\\b","replacement":"gadget",
       "filterOperationKeys":[{"operation":"generate_content","dialect":"openai_chat"}]}'
```

```json
{"id":"de487987ff3d2e6415986ca9099dd60d","ruleSetId":"f41160b45f36…",
 "phase":"request","target":"body","targetName":null,
 "paths":["messages.*.content"],"pattern":"(?i)\\bwidget\\b","replacement":"gadget",
 "filterOperationKeys":[{"dialect":"openai_chat","operation":"generate_content"}],
 "filterModelPattern":null,"filterHeaderPattern":null,"filterEventPattern":null,
 "sortOrder":0,"enabled":true,…}
```

| Field | Meaning |
| --- | --- |
| `phase` | `request`, `response` or `both`, **relative to the upstream connection**. |
| `target` | `body`, `header` or `query`. |
| `targetName` | Required for `header` and `query`, absent for `body`. Header names match case-insensitively; query names match exactly after decoding. |
| `paths` | Body only. A JSON array of dot paths selecting JSON *string values*; `null` applies the pattern to the whole payload text. |
| `pattern` | Rust regex syntax, including inline flags such as `(?i)` and `(?s)`. |
| `replacement` | Regex replacement syntax, including `$1` and `${name}`. |
| `sortOrder` | Ascending within the set; the id breaks ties. |

A rule whose pattern does not compile is refused **when you save it**, so a
stored rule never fails a request. Regexes and paths compile once per
configuration revision; execution never re-validates a row.

### Paths

`messages.*.content`, `system`, `system.*.text`, `tools.*.name`. A segment is
an object key, an array index, or `*` for every element.

The payload is **scanned, never parsed into a tree**, so key order, whitespace
and number formatting survive untouched and only the selected strings are
re-encoded. A path rule on a payload that is not JSON is a no-op rather than an
error.

With `paths: null` the pattern runs against the serialized body text. Keep such
a pattern narrow and word-bounded — on a stream it runs against every unit.

### Headers and query values

A header rule replaces within **every** value of that header, keeping repeats
and their order. A query rule does the same for one parameter, and untouched
raw segments are preserved byte for byte — only rewritten values are encoded
again.

A header whose value is not text is an error, never a lossy decode.

## Filters

All filters are ANDed; an omitted filter matches everything.

| Filter | Matches |
| --- | --- |
| `filterOperationKeys` | A JSON array of `{"operation": …, "dialect": …}` pairs. The **native** operation being executed against the provider, after conversion. |
| `filterModelPattern` | A `*` / `?` glob against the selected upstream model **or** the caller's requested name — either side matching is a hit. |
| `filterHeaderPattern` | A case-insensitive regex against the **inbound** request header lines. |
| `filterEventPattern` | A regex against the SSE event name, falling back to the JSON `type`; for a WebSocket, the JSON `type`. |

The header filter is what scopes a compatibility set to one client: a response
rule that renames tool calls for one editor would otherwise rewrite them for
every client sharing the provider. Inbound headers are a filter **condition**,
never the rewrite target — a rule that edits a header edits the one going
upstream.

The event filter needs the decoded unit, so it is evaluated when the unit
arrives rather than when rules are selected.

## Order

The provider's enabled attachments run in `(sortOrder, id)` order, and each
set's rules in the same order within it. **Visible order is execution order,
and every rule sees the previous rule's output.** There is no ordering by kind
the way v3 sorted `system_text` before `cache_breakpoint`; what you see in the
list is what happens.

## Streams

A streamed response is rewritten **per unit** — a complete SSE event, a JSON
array element, or an NDJSON record. A unit is never a network chunk, and
nothing buffers a live stream to completion.

Frames pass through byte for byte unless a rule changed their payload. For SSE
only the `data:` lines are replaced, so comments, `event:`, `id:` and `retry:`
lines and the `[DONE]` sentinel stay intact. A unit over
`settings.maxStreamEventBytes` (1 MiB by default) is an error rather than an
unbounded buffer.

## Presets

Six application-compatibility presets ship. Each makes one client application
look like a generic one to an upstream that recognises it.

```sh
curl -s http://127.0.0.1:7070/admin/api/rule-presets -H "Authorization: Bearer $GPROXY_KEY"
```

```text
opencode    OpenCode        application   37 rules
the agent   the agent-mono  application    9 rules
aider       Aider           application    2 rules
cline       Cline           application    1 rule
continue    Continue        application    1 rule
cursor      Cursor          application    1 rule
```

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/rule-sets/{id}/rule-presets/opencode \
  -H "Authorization: Bearer $GPROXY_KEY"
```

Applying a preset **replaces** the set's rules rather than merging them. A
preset is one ordered answer, and half of it interleaved with something else
rewrites text nobody predicted. To keep existing rules, read `rule-presets`,
merge the lists yourself, and `PUT` the result.

The cache presets v3 shipped are gone with the `cache_breakpoint` rule kind.
See [Prompt Caching](/guides/claude-caching/) for what v4 does instead.

## Operation Overrides

A rewrite rule cannot make a provider serve an operation it does not serve, or
send it somewhere else. Two per-provider tables do.

**`operation_rules`** override which dialects a provider speaks natively for
one operation. Channel defaults stay **in code** and are not copied into every
new provider, which is the difference from v3: there is no seeded
`channel_default` row to distinguish from an operator row, because there is no
seeded row.

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/operation-rules \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content",
       "action":"dialects","target":["claude","openai_chat"]}'
```

`action: "dialects"` with a JSON array of dialect ids in `target` replaces the
channel's declaration for that operation. What happens next follows from the
list alone:

| The client's dialect is | What runs |
| --- | --- |
| in the list | passthrough — the bytes are not converted |
| not in the list | conversion to the **first** declared dialect, and back |
| not in the list, and the operation is a stream the provider only serves buffered | convert, invoke once, then synthesize the client's own stream |
| not in the list, and nothing is declared for the operation | the provider is not a valid target for it |

The client's dialect wins whenever it is native; otherwise the first entry is
the conversion target, so the order of that array is a preference.

**`operation_endpoints`** replace the complete method URL for one
`(operation, dialect, transport)`. It is not a replacement base URL with a
default path appended — the channel's own path parameters are resolved by that
method:

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/operation-endpoints \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content","dialect":"openai_chat",
       "url":"https://elsewhere.example/v1/chat/completions"}'
```

A missing or disabled row leaves URL construction to the provider's `baseUrl`
and the channel's default path.

Both are dropped together by one reset:

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/providers/{id}/routing-defaults/reset \
  -H "Authorization: Bearer $GPROXY_KEY"
```

## The `custom` Channel's Own List

For the `custom` channel the same question has a second answer, and it is a
provider `config` key rather than an operation rule — because a `custom`
provider has no vendor whose dialects the channel could know:

```json
{ "config": { "dialects": ["openai_chat", "openai", "claude"] } }
```

An operation whose dialect is reachable from neither the list nor a conversion
fails with an error naming both sides:

```text
models.list: no conversion from OpenAi to OpenAiChat
```

That is the most common first-day surprise. If an operation fails on a `custom`
provider, check that list before looking anywhere else.
