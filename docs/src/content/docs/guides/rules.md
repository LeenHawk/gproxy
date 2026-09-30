---
title: "Rewrite rules and protocol overrides"
description: "Configure system text, caching, JSON and text edits, and upstream protocol and endpoint overrides."
---


Add rules from a provider's rewrite page, choosing a type and entering its settings. New providers receive a bound default rule set with the same name. Use advanced bindings when several providers need to share rules.

## Rule types

| Console type | Purpose | API `action` |
| --- | --- | --- |
| System text | Prepend or append text to system instructions | `system_text` |
| Cache breakpoint | Add protocol-specific global, system, message, or tool cache markers | `cache_breakpoint` |
| JSON rewrite | Set or delete a field, or merge an object | `set`, `delete`, `merge` |
| Text transform | Apply a regular expression to body text, headers, or query values | `replace` |
| Header | Set a value or merge comma-separated values | `header_set`, `header_merge` |

System text supports Claude, OpenAI Chat, Responses (including WebSocket), and Gemini. Cache breakpoints support Claude and OpenAI formats; see [Prompt caching](/guides/claude-caching/) for positions and TTLs.

## Advanced JSON editing

When creating or editing a rule within a rule set, select **Advanced JSON** to paste one v4 rule object. This is also available on a provider's rewrite page. Switch between the form and JSON, or format the JSON. Invalid input stays in the editor with an error; saving also checks that the rule can execute.

For example, replace `"type":"function"}` in the body with `"type":"function","strict":false}`:

```json
{
  "action": "replace",
  "phase": "request",
  "target": "body",
  "pattern": "\"type\":\"function\"\\}",
  "replacement": "\"type\":\"function\",\"strict\":false}"
}
```

Use the v4 `action`, `pattern`, and `replacement` fields. The page manages the rule set, ID, and order, so omit those from the JSON. Optional filters and `enabled` can be included. Omitted filters impose no restriction, and `enabled` defaults to true. Each edit accepts one rule object, not an array.

## Execution order

Requests are converted to the upstream protocol, rewritten, and then sent by the channel. Responses are rewritten before conversion back to the client protocol. Rule paths must therefore use the **upstream format**.

For example, an OpenAI Chat request routed to Claude uses Claude's `system` and `messages` fields by the time request rules run.

Enabled rule-set bindings and the rules within each set run in `sortOrder`, then ID order. Each rule sees the previous rule's output. System text and cache breakpoint rules follow the same ordering.

## Fields and regular expressions

JSON edits require paths such as `temperature` or `messages.*.content`. A `*` segment selects all elements. `set` requires a valid JSON value, `merge` requires a JSON object, and `delete` needs no value.

Text transforms use Rust regular expressions, including `(?i)`, `(?s)`, `$1`, and `${name}` replacements. With body paths, they replace selected JSON string values. Without paths, they operate on the full body text; take care to preserve valid JSON.

Header and query rules require `targetName`. `header_set` replaces the header value. `header_merge` merges comma-separated values and removes duplicates. Avoid storing upstream secrets in shared rules.

## Filters

| Filter | Matches |
| --- | --- |
| Operation and protocol | The converted operation executed on the provider |
| Model | Either the upstream or requested model name, with `*` and `?` wildcards |
| Request headers | A regular expression over inbound headers |
| Stream event | An SSE event name or JSON `type`, for body rules |

All supplied filters must match. Omitted filters add no restriction. Scope client-specific compatibility rules by request headers so they do not affect other clients.

## Streaming responses

Body rewrites operate on complete events, such as an SSE event or JSON array element, rather than network chunks. An event exceeding `settings.maxStreamEventBytes` fails. Use an event filter to target particular event types.

## Management API

This example replaces `widget` with `gadget` in selected strings. Substitute your actual rule-set ID.

```sh
curl -sS -X POST http://127.0.0.1:8787/admin/api/rules \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"ruleSetId":"your-rule-set-id","action":"replace",
       "phase":"request","target":"body","paths":["messages.*.content"],
       "pattern":"(?i)\\bwidget\\b","replacement":"gadget",
       "filterOperationKeys":[{"operation":"generate_content","dialect":"openai_chat"}]}'
```

`PUT /admin/api/rule-sets/{id}/rules` replaces a complete rule list. For system text and cache breakpoints, `replacement` contains a JSON-encoded configuration string. The console constructs it for you.

## Compatibility presets

`GET /admin/api/rule-presets` returns the built-in OpenCode, the agent-mono, Aider, Cline, Continue, and Cursor presets. Query the endpoint for the current rules.

`POST /admin/api/rule-sets/{id}/rule-presets/{preset}` **replaces** the set's existing rules. To retain existing rules, export or read them first and save a merged list.

## Protocol and endpoint overrides

A provider's routing rules select an upstream protocol. Model routes select providers; these are separate settings.

- `operation_rules` overrides a provider's protocol list for an operation. A supported client protocol passes through; otherwise, conversion targets the first protocol in the list.
- `operation_endpoints` overrides the full endpoint URL for an operation, protocol, and transport.

For example, change the protocols for generation:

```sh
curl -sS -X POST http://127.0.0.1:8787/admin/api/operation-rules \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"providerId":"your-provider-id","operation":"generate_content",
       "action":"dialects","target":["claude","openai_chat"]}'
```

Without overrides, channel defaults apply. `POST /admin/api/providers/{id}/routing-defaults/reset` clears the provider's operation and endpoint overrides.

The `custom` channel also requires a `dialects` configuration, for example `["openai_chat", "openai"]`. If an operation fails, check upstream support, this list, and operation overrides.
