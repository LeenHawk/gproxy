---
title: Dispatch Table
description: How each channel declares which (operation, protocol) pairs it serves — and whether to passthrough, transform, handle locally, or reject.
---

A **dispatch table** is the per-channel routing map. It takes an incoming
request, classified into a `(OperationFamily, ProtocolKind)` pair, and
tells GPROXY one of four things:

- **`Passthrough`** — forward the request to upstream unchanged (same
  protocol on both sides; GPROXY only rewrites the model, headers, and
  auth envelope).
- **`TransformTo { destination }`** — run the protocol `transform` layer
  to convert the request into a different `(operation, protocol)` pair
  before forwarding. The inverse transform runs on the response.
- **`Local`** — handle the request entirely inside GPROXY; never call
  upstream. Used for `model_list` / `model_get` under the `*-only`
  presets.
- **`Unsupported`** — the route is not supported on this channel; return
  501.

Each channel owns a `fn dispatch_table(&self) -> DispatchTable` method.
The value it returns is the channel's **default** routing map, and the
engine uses it unless an operator has overridden it per-provider from
the console.

Defined in [`sdk/gproxy-provider/src/dispatch.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/dispatch.rs).

## Route key

```rust
pub struct RouteKey {
    pub operation: OperationFamily,
    pub protocol:  ProtocolKind,
}
```

### Operation families

`OperationFamily` (in `gproxy-protocol`) is the protocol-agnostic kind
of call the route represents. The full list:

| Family | What it is |
| --- | --- |
| `model_list` | List available models. |
| `model_get` | Fetch one model by id. |
| `count_tokens` | Token-count endpoint. |
| `compact` | "Compact" a conversation (Claude Code compact). |
| `generate_content` | Non-streaming chat / message / generate call. |
| `stream_generate_content` | Streaming chat / message / generate call. |
| `create_image` / `stream_create_image` | Image generation. |
| `create_image_edit` / `stream_create_image_edit` | Image editing. |
| `openai_response_websocket` | OpenAI Responses WebSocket. |
| `gemini_live` | Gemini Live bidi session. |
| `embeddings` | Embedding endpoint. |
| `file_upload` / `file_list` / `file_get` / `file_content` / `file_delete` | File endpoints. |

### Protocol kinds

`ProtocolKind` is the wire dialect:

| Kind | Meaning |
| --- | --- |
| `openai` | OpenAI — umbrella used before a specific API is picked. |
| `openai_chat_completions` | OpenAI **Chat Completions** API. |
| `openai_response` | OpenAI **Responses** API. |
| `claude` | Anthropic Messages API. |
| `gemini` | Google Gemini `generateContent`. |
| `gemini_ndjson` | Gemini streaming over NDJSON (same semantics as `gemini`). |

## Example: the Anthropic channel's default table

Simplified from [`channels/anthropic.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/channels/anthropic.rs):

```rust
let pass  = |op, proto| (RouteKey::new(op, proto), RouteImplementation::Passthrough);
let xform = |op, proto, dst_op, dst_proto| (
    RouteKey::new(op, proto),
    RouteImplementation::TransformTo {
        destination: RouteKey::new(dst_op, dst_proto),
    },
);

let routes = vec![
    // Native Claude routes — passthrough.
    pass(OperationFamily::GenerateContent, ProtocolKind::Claude),
    pass(OperationFamily::StreamGenerateContent, ProtocolKind::Claude),

    // OpenAI Chat Completions clients are transformed into Claude on the way in.
    xform(
        OperationFamily::GenerateContent,   ProtocolKind::OpenAiChatCompletion,
        OperationFamily::GenerateContent,   ProtocolKind::Claude,
    ),
    // …
];
```

Read the table left-to-right as *"when the **client** sends a
`(operation, protocol)`, do X"*. The destination in a `TransformTo`
entry is what the **upstream** ends up seeing.

## Passthrough vs. Transform

The difference is a hot-path one:

- **`Passthrough`** is the minimal-parsing fast path. The request body
  is forwarded with only the model, headers, and auth touched. This is
  why same-protocol routing is so cheap — see
  [Providers & Channels](/guides/providers/).
- **`TransformTo`** runs the protocol conversion in
  `gproxy_protocol::transform`, producing a new body that the upstream
  can accept. The inverse transform runs on the response (including
  per-chunk rewriting for streams).

## `Local` dispatch for model lists

The `*-only` presets (for example `chat-completions-only`,
`response-only`, `claude-only`, `gemini-only`) set `model_list` and
`model_get` to `Local`. Those requests are answered entirely from the
local `models` table — GPROXY never calls upstream.

Concretely, when a channel's dispatch table maps a route to `Local`,
the handler calls `Channel::handle_local(...)` on the channel. That
function builds the protocol-specific response body directly from the
in-memory models snapshot.

For `*-like` / pass-through presets, `model_list` stays as
`Passthrough`, and GPROXY **merges** the upstream response with the
local `models` table after the call returns (see
[Models & Aliases](/guides/models/) for why and how).

## Why operators care

Most users never touch the dispatch table — the defaults from each
channel are fine. You'd override it when you want to:

- **Force local model listing** on a channel whose upstream returns a
  huge list you don't want clients to see.
- **Disable** (`Unsupported`) a feature your upstream technically
  supports but you don't want exposed (for example, blocking image
  generation on an OpenAI provider).
- **Cross-wire** an odd shape — e.g. redirect OpenAI Responses traffic
  into a Chat Completions upstream by transforming the operation.

Provider-level overrides are edited from the console's provider
workspace and are stored as JSON (`DispatchTableDocument`). The schema
enforces one entry per `(operation, protocol)` pair.
