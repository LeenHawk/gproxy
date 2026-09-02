---
title: Routing Rules & Rule Sets
description: "Rule sets mutate provider-native requests and responses; routing rules decide how each provider serves an operation and inbound protocol"
---

GPROXY has two rule mechanisms. Both are edited in the console and both live
in the control plane:

- **Rule sets** are reusable, ordered lists of mutation rules. A set is
  attached to one or more providers and edits the provider-native request
  before it leaves, and the provider-native response before it is converted
  back for the client.
- **Routing rules** belong to one provider. For each operation and inbound
  protocol they say whether the provider passes the request through,
  transforms it to another wire format, answers it locally, or refuses it.

The global **Rules** workspace (`/admin/rules`) edits rule sets and their
attachments. A provider's **Rules** tab edits the same objects scoped to that
provider and offers presets; its **Routing** tab edits routing rules.

## Where Rules Run

```text
client request
  -> classify operation and inbound protocol
  -> select provider and credential
  -> routing rule: passthrough / transform_to / local / unsupported
  -> protocol transform to the provider's native format
  -> rule sets, request phase (system_text -> cache_breakpoint -> rewrite
     -> transform -> header)
  -> Claude magic-string cache pass (if enabled on the provider)
  -> channel prepare: URL, auth, forwarded metadata
upstream response
  -> channel shaping
  -> rule sets, response phase (transform only), on 2xx bodies
  -> protocol transform back to the client's format
```

Rules therefore see the upstream's wire shape. An OpenAI Chat request routed
to a Claude provider is converted to Claude Messages first, and its rules
edit `system`, `messages[]` and `tools[]`, not `messages[].role`.

Streamed responses are edited frame by frame: each SSE `data:` payload or
Gemini JSON-array element is rewritten independently. The `[DONE]` sentinel
is left alone. Response rules do not apply to WebSocket sessions.

## Rule Sets

A rule set has a `name`, an optional `description` and an `enabled` flag.
Disabled sets, disabled rules and disabled attachments are all skipped at
compile time. A rule whose configuration does not compile is rejected when
you save it, so a stored rule never fails a request.

### Rule Fields

Every rule is written to `POST /admin/api/rules` in this shape:

```json
{
  "rule_set_id": 4,
  "config": { "kind": "system_text", "text": "...", "position": "prepend" },
  "filter_model_pattern": "claude-*",
  "filter_operations": ["generate_content", "stream_generate_content"],
  "filter_header_pattern": "^user-agent: opencode/",
  "sort_order": 0,
  "enabled": true
}
```

| Field | Meaning |
| --- | --- |
| `config.kind` | `system_text`, `cache_breakpoint`, `rewrite`, `transform` or `header`; the rest of `config` depends on the kind. |
| `filter_model_pattern` | Glob (`*`, `?`) matched against the whole upstream model id, and also against the client's requested name (route, alias or variant) when it differs. |
| `filter_operations` | Stable operation ids such as `generate_content`, `stream_generate_content`, `list_models`, `create_embedding`. |
| `filter_header_pattern` | Case-insensitive regex tested against each inbound header rendered as `name: value`; the rule applies when any line matches. |
| `sort_order` | Declaration order inside the set. |

Filters are ANDed; an omitted filter matches everything. The header filter
is what scopes an application-compatibility set to one client: a response
rule that renames tool calls for OpenCode would otherwise rewrite them for
every client sharing the provider. Read the exact header lines a client
sends from **Request audit** ([Usage, Logs & Audit](/guides/observability/)).

### `system_text`

Prepends or appends server-managed text at the native system-instruction
location of the target format. It does nothing for operations that are not
content generation.

```json
{ "kind": "system_text", "text": "Follow the workspace policy.", "position": "prepend" }
```

| Target format | Location |
| --- | --- |
| Claude Messages | `system` string, or a text block inserted into `system[]` |
| OpenAI Chat Completions | a `role: "system"` message, first or after the existing leading system messages |
| OpenAI Responses (HTTP and WebSocket) | `instructions` |
| Gemini GenerateContent | `systemInstruction.parts[]` |

### `cache_breakpoint`

Places a native cache marker: Claude `cache_control`, or OpenAI
`prompt_cache_breakpoint` / `prompt_cache_options`. Gemini targets are
skipped.

```json
{ "kind": "cache_breakpoint", "target": "system", "index": null, "ttl": "1h" }
```

Targets are `top_level`, `system`, `tools` (Claude only) and `message`.
Positions, TTLs and the marker cap are described in
[Prompt Caching](/guides/claude-caching/).

### `rewrite`

Edits one JSON path in the request body. Paths are dot-separated; segments
are object keys or numeric array indexes (`messages.0.content`).

```json
{ "kind": "rewrite", "path": "stream_options.include_usage", "action": "set", "value": true }
```

| Action | Behaviour |
| --- | --- |
| `set` | Creates missing object parents and writes `value` at the leaf; an array index must already exist. |
| `delete` | Removes the key or array element; a missing path is skipped. |
| `merge` | Shallow-merges an object `value` into the existing object at the path. |

The console's model-variant editor stores variants as `rewrite`/`set` rules
whose model filter is the variant name, so a thinking-level variant is just
a rule you can inspect.

### `transform`

Text replacement over selected JSON string values, or over the serialized
body. It is the only kind with a response phase.

```json
{
  "kind": "transform",
  "phase": "request",
  "locate": { "type": "paths", "value": ["tools.*.name", "tool_choice.name"] },
  "actions": [{ "op": "replace_regex", "pattern": "^mcp_([^_].*)$", "with": "mcp__$1" }],
  "limit": null
}
```

| Field | Values |
| --- | --- |
| `phase` | `request` (default), `response`, `both` |
| `locate` | `{"type":"path","value":"a.*.b"}`, `{"type":"paths","value":[...]}` with `*` wildcards, or `{"type":"match","value":"<regex>"}` over the serialized body |
| `actions[].op` | `replace_text` with `with` and an optional exact `from` guard; `replace_regex` with a Rust `pattern` and `with` (`$1` groups) |
| `limit` | Maximum matched values (path locators) or replacements (body match) |

A body `match` locator accepts only `replace_text`; on streams it runs
against each frame, so keep the pattern narrow and word-bounded.

### `header`

Sets or merges an outgoing request header.

```json
{ "kind": "header", "name": "anthropic-beta", "value": "extended-cache-ttl-2025-04-11", "mode": "merge" }
```

`override` (default) replaces the header. `merge` appends a comma-separated
value and skips it when already present. Header rules run before the
provider's forwarded-metadata policy, so the header must be one the channel
forwards (**Providers → Settings → Forwarded metadata**).

### Fixed Apply Order

```text
system_text -> cache_breakpoint -> rewrite -> transform -> header
```

Order is by kind first, regardless of which set a rule came from. Within a
kind, attached sets run in attachment order and rules in `sort_order`. The
console shows the resulting "Effective #" beside each rule.

### Attaching Sets to Providers

An attachment (`POST /admin/api/provider-rule-sets`) links a set to a
provider with its own `sort_order` and `enabled` flag. The Rules workspace
labels each set **Unused**, **One provider** (private) or **Shared** by its
attachment count. A set cannot be deleted while it still has rules or
attachments.

Creating a provider also creates and attaches an empty private set named
`<provider> · defaults`. The console writes model-variant rules there; you
may add your own rules to it as well.

### Presets

`GET /admin/api/rule-presets` lists presets; a provider's Rules tab applies
one with **Apply compatibility preset**
(`POST /admin/api/providers/<id>/rule-presets/<preset>`). Applying creates
or updates an ordinary set named `<preset> compatibility`, attaches it and
leaves it editable. Applying again refreshes the rules in place.

| Preset | Category | What it does |
| --- | --- | --- |
| OpenCode | application | Strips the `<env>` block and OpenCode branding from `system`; renames lowercase tool names to Claude Code's TitleCase and `mcp_` to `mcp__` on the request, and back on the response. Scoped to `^user-agent: opencode/`. |
| pi-mono | application | Rewrites "pi" agent branding in prompt text. No header filter. |
| Aider | application | Rewrites "Aider" branding; scoped to `^user-agent: litellm/`. |
| Cline | application | Rewrites "Cline"; scoped to Cline's user agent, `x-title` or `http-referer`. |
| Continue | application | Rewrites "Continue"; scoped to `^user-agent: continue/`. |
| Cursor | application | Rewrites "Cursor"; scoped to `^user-agent: cursor/`. |
| Claude system cache | cache | `cache_breakpoint` on the last `system` block, TTL `1h`. |
| Claude message cache | cache | `cache_breakpoint` on the last cacheable message block, TTL `1h`. |

Application presets filter on `generate_content` and
`stream_generate_content` and operate on request text paths only.

## Routing Rules

A routing rule belongs to one provider and is keyed by `operation` and
inbound `kind`: a wire family or a content-generation protocol, for example
`openai`, `claude`, `gemini`, `openai_chat`, `openai_responses`,
`openai_responses_websocket`, `claude_messages`, `gemini_generate_content`.

```json
{
  "provider_id": 3,
  "operation": "generate_content",
  "kind": "claude_messages",
  "implementation": "transform_to",
  "dest_operation": "generate_content",
  "dest_kind": "openai_chat",
  "sort_order": 0,
  "enabled": true
}
```

| `implementation` | Effect |
| --- | --- |
| `passthrough` | Send the request in the inbound format; the channel must speak it natively. |
| `transform_to` | Convert to `dest_operation` + `dest_kind` before the channel sees it, and convert the response back. Both `dest_*` fields are required and must name a wired transform pair. |
| `local` | GPROXY answers from its own state (model list, model get, token counting) and never calls this provider for the operation. |
| `unsupported` | This provider refuses the operation in this protocol. |

### Channel Defaults and Operator Rows

Each channel declares a default table in code. Creating a provider seeds one
row per entry with origin `channel_default`; the console shows these rows
muted and labels them **Inherited**. Editing a row, adding one or deleting
one turns it into an operator row. Startup backfills new channel defaults
for existing providers without touching rows that already exist.

**Reset defaults** (`POST /admin/api/providers/<id>/routing-defaults/reset`)
deletes every routing rule of the provider and reseeds the channel table.

### What Unsupported Means

When the resolved rule for a request is `unsupported` or `local`, or when
the channel declares no route for the inbound (operation, protocol) at all,
the provider is not a valid target: the executor skips it and moves to the
next route member, and a client whose route has no capable member receives
an unsupported-operation error. Rules with `enabled: false` are ignored and
the channel's own declaration applies instead.
