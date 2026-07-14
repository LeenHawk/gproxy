---
title: Rewrite Rules
description: Reuse request rules across providers to add instructions, cache breakpoints, field changes, text replacements, and headers.
---

Use **rule sets** when requests need a consistent adjustment before they reach
an upstream. A set can be attached to one or more providers, so common behavior
does not need to be configured repeatedly.

Rules see the request in the upstream's API format. For example, an OpenAI
request routed to Claude is converted to Claude Messages before its rules run.

```text
client request
  -> authenticate and choose an upstream
  -> convert to the upstream API format
  -> apply provider rule sets
  -> send the upstream request
```

The console lets you create a named set, add ordered rules, and attach the set to
providers. Disabled or invalid rules are skipped instead of failing requests.

## Common Rule Fields

Every rule has:

- `kind`: one of `system_text`, `cache_breakpoint`, `rewrite`, `transform`,
  `header`.
- `config_json`: the settings for that rule kind.
- `filter_model_pattern`: optional glob against the prefix-stripped upstream
  model name.
- `filter_operation_keys`: optional list of `Operation` values such as
  `generate_content` or `stream_generate_content`.
- `sort_order` and `enabled`.

Filters are ANDed. Omitted filters match everything.

## `cache_breakpoint`

`cache_breakpoint` inserts the native cache marker for the selected upstream
format. Claude uses `cache_control`; OpenAI Chat and Responses use
`prompt_cache_breakpoint`.

```json
{
  "target": "system",
  "ttl": "30m"
}
```

OpenAI supports `system` and `message` content, plus request-wide behavior
through `top_level`; it does not support breakpoints on `tools`. Claude supports
`top_level`, `system`, `tools`, and `message`. See
[Prompt Caching](/guides/claude-caching/) for the complete target, TTL, and
magic-string behavior.

## `rewrite`

`rewrite` mutates a JSON body path:

```json
{
  "path": "stream_options.include_usage",
  "action": "set",
  "value_json": true
}
```

Supported actions are:

| Action | Behavior |
| --- | --- |
| `set` | Creates missing object parents and writes `value_json` at the leaf. |
| `delete` | Removes an object key or array element if present. Missing paths are skipped. |
| `merge` | Shallow-merges an object `value_json` into an existing object at the path. |

Paths are dot-separated. Object keys and numeric array indexes are supported,
for example `messages.0.content`. A missing path is skipped without breaking the
request.

## `transform`

`transform` applies generic text replacements over matched JSON paths or over
the serialized body with Rust regex:

```json
{
  "phase": "request",
  "locate": { "match": "\\binternal-tool\\b" },
  "actions": [{ "op": "replace_text", "with": "tool" }]
}
```

For structural values, use `locate.path` with dot-separated segments, `*`
wildcards, and an optional exact `from` guard:

```json
{
  "phase": "response",
  "locate": { "path": "content.*.name" },
  "actions": [{ "op": "replace_text", "from": "tasklist", "with": "todowrite" }]
}
```

`phase` is `request`, `response`, or `both`; it defaults to `request`. Regex
matching is broad after JSON serialization, so keep patterns precise.

## `header`

`header` sets or merges a request header:

```json
{
  "name": "anthropic-beta",
  "value": "extended-cache-ttl-2025-04-11",
  "mode": "merge"
}
```

`override` replaces the header. `merge` appends a comma-separated value and
removes duplicates, which is useful for headers such as `anthropic-beta`.

## Fixed Apply Order

Rules apply in this fixed order, regardless of attachment order:

```text
system_text -> cache_breakpoint -> rewrite -> transform -> header
```

Within each kind, set and rule order is preserved. A rule that cannot apply is
skipped and logged, rather than failing the upstream request.
