---
title: Rewrite Rules
description: Use v2 rule sets to mutate provider-native JSON bodies and headers after protocol transform and before upstream send.
---

Rewrite rules in v2 live in reusable **rule sets**. A rule set can attach to one
or more providers through `provider_rule_sets`, and the provider's attached
rules run after protocol transform and before channel request preparation.

```text
client request
  -> classify / auth / route / balance
  -> protocol transform to provider-native body
  -> process rule sets
  -> channel shape_request
  -> channel prepare / upstream send
```

This page covers the current specialized rule kinds. The intended design
direction is a single generic `locate + actions (+ limit)` engine with console
presets for provider-specific tasks. Until that lands, v2 uses the rule kinds
below.

## Rule Set Model

| Record | Purpose |
| --- | --- |
| `rule_sets` | Named reusable collection. |
| `rules` | Rule rows inside a set. |
| `provider_rule_sets` | Attachment of a set to a provider with `sort_order`. |

During snapshot rebuild, enabled rule sets are compiled. Unparsable rules warn
and skip. Provider attachments are flattened in attachment order, then sorted by
fixed kind order.

## Common Rule Fields

Every rule row has:

- `kind`: one of `system_text`, `cache_breakpoint`, `rewrite`, `sanitize`,
  `header`.
- `config_json`: kind-specific config.
- `filter_model_pattern`: optional glob against the prefix-stripped upstream
  model name.
- `filter_operation_keys`: optional list of `Operation` values such as
  `generate_content` or `stream_generate_content`.
- `sort_order` and `enabled`.

Filters are ANDed. Omitted filters match everything.

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
for example `messages.0.content`. This is intentionally simple and fail-soft.

## `sanitize`

`sanitize` applies a Rust regex replacement over the serialized request body:

```json
{
  "pattern": "\\binternal-tool\\b",
  "replacement": "tool"
}
```

It is broad and byte-level after JSON serialization, so use precise patterns.
For structured changes, prefer `rewrite`. In the future generic model,
sanitization maps to `locate.match + replace_text`.

## `header`

`header` sets or merges a request header:

```json
{
  "name": "anthropic-beta",
  "value": "extended-cache-ttl-2025-04-11",
  "mode": "merge"
}
```

`override` replaces the header. `merge` comma-appends with de-duplication, which
is useful for list-valued headers such as `anthropic-beta`.

## Fixed Apply Order

Rules apply in this fixed order, regardless of attachment order:

```text
system_text -> cache_breakpoint -> rewrite -> sanitize -> header
```

Within each kind, set and rule sort order is preserved. A bad or non-applicable
rule should not break traffic; it warns and skips.
