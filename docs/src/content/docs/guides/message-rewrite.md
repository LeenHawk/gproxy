---
title: Message Rewrite
description: Add system instructions or replace prompt text without changing client applications.
---

Message rewrite rules let the gateway adjust prompt text without requiring every
client application to make the same change. Choose the rule type that matches
what you need:

- `system_text` adds text to the upstream format's system-instruction location.
- `transform` replaces text patterns over the serialized body or matched paths.
- `rewrite` edits a specific JSON path when you know the upstream API shape.

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

## `transform`

Use `transform` for regex replacement when the exact structural path is not the
right model:

```json
{
  "phase": "request",
  "locate": { "match": "\\bAcme internal\\b" },
  "actions": [{ "op": "replace_text", "with": "the workspace" }]
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

Limit message rules to content-generation operations so they do not run on
model-list, embedding, or image requests:

```json
["generate_content", "stream_generate_content"]
```

Add a model filter as well when the rewrite is intended for only one model or
model family.

## Caching Interaction

Claude and OpenAI prompt caches match an exact prefix. If a rewrite changes text
before `cache_control` or `prompt_cache_breakpoint`, an expected cache hit can
become a miss. Apply rewrites to stable content first, then place the cache
breakpoint after the rewritten prefix.
