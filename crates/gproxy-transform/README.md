# gproxy-transform

[![crates.io](https://img.shields.io/crates/v/gproxy-transform.svg)](https://crates.io/crates/gproxy-transform)
[![docs.rs](https://docs.rs/gproxy-transform/badge.svg)](https://docs.rs/gproxy-transform)

Pure, synchronous pairwise protocol transforms for OpenAI, Anthropic Claude,
and Google Gemini. The crate contains no HTTP client, runtime, routing policy,
or server framework.

## Typed pairs

Call typed pairs directly when protocol models are already available:

```rust
use gproxy_transform::protocol::openai;
use gproxy_transform::typed::{RequestContext, generate_content};

fn convert(
    request: openai::ChatCompletionRequest,
) -> Result<gproxy_transform::protocol::claude::CreateMessageRequestBody,
            gproxy_transform::TransformError> {
    generate_content::openai_chat_to_claude_messages::request(
        request,
        RequestContext::new("claude-sonnet-4-6", false),
    )
}
```

Every content pair also exposes a stateful typed stream converter under
`typed::stream`. One source event may produce zero or many target events.

## Bytes and framing

`request`, `response`, `ResponseStream`, and `ResponseCollector` are the dynamic
Bytes facade used by GPROXY. They parse once, call the same typed core, and
serialize once. `synthesize_response` turns a complete response into a strict
SSE, Gemini JSON-array, or Responses WebSocket stream.

Transforms are direct pairs with no intermediate representation. Same-wire
traffic bypasses semantic conversion. Unknown source extensions never leak into
another provider's wire shape.

## License

Licensed under the [MIT License](LICENSE).
