---
title: Message Rewrite
description: Add or replace message text with v2 system_text, sanitize, and generic rewrite rule-set entries.
---

v2 does not have a separate "message rewrite" table. Message-oriented rewriting
is expressed through the provider rule-set system:

- `system_text` inserts text into the provider-native system location.
- `sanitize` replaces text patterns over the serialized body.
- `rewrite` edits explicit JSON paths when you know the provider-native shape.

These rules run after protocol transform. That matters: a request from an OpenAI
client routed to Claude is first transformed to Claude Messages, then message
rules see the Claude body shape.

## `system_text`

Use `system_text` to prepend or append server-managed instructions:

```json
{
  "text": "Follow the internal safety policy for this workspace.",
  "position": "prepend"
}
```

Supported positions are `prepend` and `append`. The runtime maps the insertion
to the selected content-generation kind:

| Target kind | Native location |
| --- | --- |
| `claude_messages` | `system` string or `system[]` text block. |
| `open_ai_chat_completions` | A `messages[]` item with `role: "system"`. |
| `open_ai_responses` | `instructions`. |
| `gemini_generate_content` | `systemInstruction.parts[]`. |

This is one of the few current rule kinds that knows protocol semantics. The v2
design preference is to move this kind of provider-specific path choice into
frontend/config presets once the generic transform engine exists.

## `sanitize`

Use `sanitize` for regex replacement when the exact structural path is not the
right model:

```json
{
  "pattern": "\\bAcme internal\\b",
  "replacement": "the workspace"
}
```

The replacement runs over the serialized provider-native request body. It can
modify text anywhere in the body string representation. That power is useful for
prompt text, but it can also affect JSON string values you did not intend to
touch. Prefer word boundaries and narrow patterns.

## `rewrite`

Use `rewrite` when you know the provider-native path:

```json
{
  "path": "messages.0.content",
  "action": "set",
  "value_json": "Pinned instruction text"
}
```

This is exact and structural, but it is not portable across protocol kinds. A
Claude system path, an OpenAI Chat system message, an OpenAI Responses
`instructions` string, and a Gemini `systemInstruction` object are different
structures.

## Scope Rules by Operation

Message rewrite rules should usually filter on content-generation operations:

```json
["generate_content", "stream_generate_content"]
```

Avoid provider-family filters as the organizing concept. v2 classifies behavior
by `Operation` and `OperationGroup`; the provider or protocol kind is a wire
shape used inside that operation.

## Caching Interaction

Claude prompt cache keys depend on exact prefix content. If a message rewrite
changes text before a `cache_control` breakpoint, it can turn every request into
a cache miss. Put stable cache breakpoints after rewritten content, or keep
rewrite rules outside the cached prefix.
