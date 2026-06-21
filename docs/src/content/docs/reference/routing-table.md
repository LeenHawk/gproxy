---
title: Routing Table
description: How v2 stores per-provider routing rules for passthrough, transform, local handling, and unsupported operations.
---

In GPROXY v2, routing is a persisted per-provider matrix. Each row maps an
incoming `(operation, kind)` pair to one implementation:

- `passthrough` forwards to the selected provider without changing the wire
  dialect;
- `transform_to` converts the request to another operation/kind before sending
  it upstream and converts the response back when supported;
- `local` handles the request inside GPROXY without an upstream call;
- `unsupported` rejects the cell.

At request time, only stored and enabled rows are consulted. If no row matches,
the request is unsupported. Channel defaults are materialized into real
`routing_rules` rows when a provider is created or when an operator resets the
provider's routing rules.

## Stored row shape

`routing_rules` rows are scoped to one provider:

| Field | Description |
| --- | --- |
| `provider_id` | Provider whose channel and credentials will service the request. |
| `operation` | Provider-neutral operation string, for example `generate_content`, `stream_generate_content`, `list_models`, `count_tokens`, `create_image`, or `create_embedding`. |
| `kind` | Inbound wire kind. Content generation uses concrete dialect names such as `open_ai_responses`, `open_ai_chat_completions`, `claude_messages`, or `gemini_generate_content`. Other operations use provider families such as `open_ai`, `claude`, or `gemini`. |
| `implementation` | `passthrough`, `transform_to`, `local`, or `unsupported`. |
| `dest_operation` | Destination operation for `transform_to`; may be omitted to keep the same operation. |
| `dest_kind` | Destination wire kind for `transform_to`. A transform row without `dest_kind` is treated as unsupported. |
| `sort_order` | Ordering used when compiling enabled rows. The effective key is still unique per `(provider_id, operation, kind)`. |
| `enabled` | Disabled rows are ignored. |

The database enforces uniqueness for `(provider_id, operation, kind)`.

## Operation vocabulary

Current operation enum values are:

| Operation | Group | Notes |
| --- | --- | --- |
| `list_models`, `get_model` | Models | Model list/get endpoints. |
| `count_tokens` | Count tokens | Provider token counting endpoints. |
| `generate_content`, `stream_generate_content` | Generate content | OpenAI Chat Completions, OpenAI Responses, Claude Messages, and Gemini generateContent dialects. |
| `create_image`, `edit_image` | Images | OpenAI-shaped image generation/edit operations; transforms exist only where implemented. |
| `create_embedding` | Embeddings | OpenAI and Gemini embedding shapes. |
| `compact_content` | Compact | Compact endpoint used by agent workflows. |
| `create_conversation` | Conversation | OpenAI conversation-shaped operation. |

Content-generation operations must use a content-generation kind. Non-content
operations use a provider family kind.

## Example rows

Passthrough OpenAI Responses traffic to an OpenAI provider:

```json
{
  "provider_id": 1,
  "operation": "generate_content",
  "kind": "open_ai_responses",
  "implementation": "passthrough",
  "dest_operation": null,
  "dest_kind": null,
  "sort_order": 0,
  "enabled": true
}
```

Accept Claude Messages from a client and transform them to OpenAI Responses
upstream:

```json
{
  "provider_id": 1,
  "operation": "generate_content",
  "kind": "claude_messages",
  "implementation": "transform_to",
  "dest_operation": "generate_content",
  "dest_kind": "open_ai_responses",
  "sort_order": 10,
  "enabled": true
}
```

Answer model listing locally:

```json
{
  "provider_id": 1,
  "operation": "list_models",
  "kind": "open_ai",
  "implementation": "local",
  "dest_operation": null,
  "dest_kind": null,
  "sort_order": 20,
  "enabled": true
}
```

## Default seeding and reset

Every channel exposes a default routing table in code. Creating a provider
through the admin API seeds the channel's declared cells into `routing_rules`.
The reset endpoint recomputes defaults for the provider's channel:

```text
POST /admin/providers/{provider_id}/routing-rules/reset
```

Reset overwrites declared default cells. It does not invent support for cells
the channel does not declare, and rows outside the channel defaults are left to
operator configuration.

Raw JSON bundle import does not call provider creation helpers. If a bundle
needs routing rows, include the `routing_rules` array explicitly or reset the
provider from the admin API after import.

## Request flow

1. The HTTP path is classified into an `OperationKey`.
2. Route or scoped-provider selection chooses candidate providers and upstream
   model ids.
3. The provider's enabled `routing_rules` are compiled.
4. The dispatch decision is applied:
   - `passthrough`: preserve the inbound request target/body shape;
   - `transform_to`: synthesize the destination provider-relative target and
     run the transform layer;
   - `local`: call the channel's local handler;
   - missing row or `unsupported`: return an unsupported-operation error.

## Routes versus routing rules

`routes` and `route_members` decide which provider/model candidate should serve
a logical model name. `routing_rules` decide whether that chosen provider can
serve the inbound wire operation and how the request must be shaped.

For example, an alias may resolve `default-chat` to route `main`, and route
`main` may choose provider `openai-main` model `gpt-4.1-mini`. The provider's
`routing_rules` then decide whether a Claude Messages request can be transformed
to the OpenAI upstream dialect for that candidate.
