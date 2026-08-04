# gproxy-transform

[![crates.io](https://img.shields.io/crates/v/gproxy-transform.svg)](https://crates.io/crates/gproxy-transform)
[![docs.rs](https://docs.rs/gproxy-transform/badge.svg)](https://docs.rs/gproxy-transform)
[![license](https://img.shields.io/crates/l/gproxy-transform.svg)](LICENSE)

Convert requests, responses, and streaming events between the OpenAI, Anthropic
Claude, and Google Gemini APIs.

`gproxy-transform` is the transform layer of
[GPROXY](https://github.com/LeenHawk/gproxy), built on the wire types in
[`gproxy-protocol`](https://crates.io/crates/gproxy-protocol). It is a pure,
synchronous library: no HTTP client, no runtime, no I/O.

## Usage

```toml
[dependencies]
gproxy-transform = "=2.3.0"
```

Resolve a pair from the source/target operation keys, then run the bytes-level
dispatch:

```rust
use gproxy_transform::protocol::{ContentGenerationKind, Operation, OperationKey};
use gproxy_transform::{TransformContext, dispatch, resolve};

let source = OperationKey::content_generation(
    Operation::GenerateContent,
    ContentGenerationKind::OpenAiChatCompletions,
);
let target = OperationKey::content_generation(
    Operation::GenerateContent,
    ContentGenerationKind::ClaudeMessages,
);

// Request direction: inbound OpenAI chat body → upstream Claude messages body.
let pair = resolve(source, target)?;
let ctx = TransformContext::new(source, target).with_request("/v1/chat/completions", None);
let converted = dispatch::request_bytes_detailed(pair, &ctx, inbound_body)?;
let upstream_body = converted.value;
let semantic_losses = converted.diagnostics;

// Response direction uses the REVERSE pair.
let back = resolve(target, source)?;
let back_ctx = TransformContext::new(target, source);
let inbound_body = dispatch::response_bytes(back, &back_ctx, upstream_body)?;

// Streaming is stateful and one input event may yield zero or many outputs.
let mut stream = dispatch::StreamConverter::new(back, back_ctx)?;
let inbound_events = stream.push(upstream_frame_data)?;
let final_events = stream.finish()?;
```

## Design

- **Pairwise, no intermediate representation.** Every conversion is a direct
  `source → target` module. This keeps provider-specific quirks in the one file
  that owns them instead of leaking into a shared IR.
- **Organized by operation, not by provider.** `generate_content`,
  `count_tokens`, `models`, `embeddings`, `images`, `compact`.
- **Same-kind traffic never enters this crate.** Route it as passthrough.
- **`extra` fields are not preserved.** Source `extra` is dropped; target
  `extra` starts empty.
- **Streaming is `0..N` and stateful.** Retain one `dispatch::StreamConverter`
  (or `stream_adapter::SseTransformer`) for the whole response so multi-part
  frames and tool calls split across frames are preserved.
- The default SSE adapter is strict: malformed/oversized frames and abnormal
  EOF return errors and do not synthesize a successful terminator. Lenient
  skipping is an explicit `StreamOptions` choice.
- `request_bytes_detailed`, `response_bytes_detailed`, `push_detailed`, and
  `finish_detailed` return structured non-fatal semantic-loss diagnostics.
- Routing tables, database rules, and logging policy belong to the host;
  `gproxy-transform` contains no routing-policy module.

Use `dispatch::is_wired` to check whether a resolved pair has a bytes-level
implementation before routing traffic through it.

## Modules

| Module           | Purpose                                                      |
| ---------------- | ------------------------------------------------------------ |
| `dispatch`       | Bytes-level `(pair, ctx, body) -> body` entry points          |
| `generate_content` | Chat/messages/generateContent pairs, streaming included     |
| `count_tokens`   | Token-count request/response pairs                            |
| `models`         | Model list/get pairs                                          |
| `embeddings`     | Embedding pairs                                               |
| `images`         | Image create/edit pairs                                       |
| `compact`        | Context-compaction pairs                                      |
| `stream_adapter` | Runtime SSE adapter (decode → convert → re-encode)            |
| `common`         | Mechanical helpers only (SSE framing, roles, tool ids, usage) |

## License

Licensed under the [MIT License](LICENSE).
