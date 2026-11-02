# gproxy-client-sdk

A caller-facing SDK for programs calling GPROXY over HTTP or through an embedded
`gproxy-sdk::Gproxy`. The IR belongs to this crate; protocol's pairwise conversions
are unchanged. HTTP and SSE reuse `gproxy-client`, including wasm fetch.

```rust,no_run
use gproxy_client_sdk::{backend::Remote, *};

# async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
let client = Client::new(Remote::new("https://gproxy.example", "gateway-key").await?);
let mut request = GenerateRequest::new(
    Model { id: "provider/model".into(), dialect: Dialect::Openai },
    vec![Message { role: Role::User, content: vec![Block::text("Hello")] }],
);
request.provider_options.openai.responses.store = Some(Some(false));
let result = client.stream(&request).await?.aggregate().await?;
let history = Conversation::new(vec![result.into_message()]);
let persisted = serde_json::to_string(&history)?;
# Ok(())
# }
```

`Model.dialect` is explicit: Claude Messages, OpenAI Responses (`Openai`), OpenAI
Chat Completions (`OpenaiChat`), or Gemini GenerateContent. The local model catalog
and its permissions are unchanged; `owned_by` does not identify a dialect.
For Claude, set `settings.max_output_tokens` or `provider_options.claude.max_tokens`.

Use `Client::new(backend::Embedded::new(engine, scope))` on native targets for the
same API. The embedding host configures the engine and owns access policy.

## Content and persistence

Every `ContentBlock<T>` has vendor-scoped `provider_metadata` and an optional
cache breakpoint. `Block` is the general `ContentBlock<Content>` alias.

| Content | Fields and behavior |
|---|---|
| Text / Reasoning | Text; Text also has portable citations |
| Image | Source, MIME and detail; supports model-generated images |
| Document | PDF/text/assets/custom chunks, title/context, citation settings, detail |
| File | Other attachments, MIME and filename |
| Audio | Source, format and transcript; PCM requires sample rate and channels |
| Video | Source, MIME, start/end seconds and fps |
| ToolCall / ToolResult | ID/name, original arguments or JSON/media output, provider execution flag |
| Opaque | Native value without a portable meaning; replayed only to its vendor |

Tool results contain JSON or restricted Text/Image/Document blocks. Custom
Document chunks contain Text/Image blocks. Citations reference a URL or a document
with page/character/chunk ranges. Native document indices/file IDs remain vendor
scoped when the response does not include the source document.

Persist `Conversation::V1`: `{"schema_version":"1","messages":[...]}`. This round
changes V1 and its metadata in place, as authorized before any caller persisted
it. From this point, incompatible changes need a new version and migration;
compatible additions need serde defaults. Unknown versions fail to decode.

## Metadata, options and native mapping

Renderers build protocol structs/enums directly. Parsers match those types.
SDK-owned metadata structs retain signatures, encrypted/redacted reasoning, native
item IDs/status, citations' native anchors and unknown `Rest` fields. Portable
text, tool arguments and media payloads stay in IR. Layout metadata preserves
Responses message grouping and reasoning segments, Chat reasoning detail segments,
and Gemini transcription segments. Server tool outputs use IR ToolResult values.
Full native values are stored only for Opaque blocks.

`ProviderOptions` contains SDK-owned `ClaudeOptions`, `OpenaiOptions` (separate
`responses` and `chat` settings), and `GeminiOptions`. Their known fields use
protocol's existing types. Each has a `rest` remainder for extensions. Portable
settings override their corresponding native controls; vendor tools supplement
portable tools. Nested native extension fields are retained. Use `rest` only for
fields outside the SDK-owned option schema.

When the vendor reference declares an **optional** field nullable, missing and
null are equivalent. Renderers may omit null when protocol cannot emit it. Required
nullable Claude citation titles and count-token context metadata remain null.
The SDK uses the supplied protocol fixes for these fields; it makes no protocol
changes. JSON decoding is limited to serialization boundaries, schema-free tool
payloads, and typed extension readers for fields in native `Rest` (such as Chat
audio chunks). There is no generated projection or JSON overlay of modeled data.

When changing vendors, other vendors' metadata/options, signatures, encrypted or
redacted reasoning, native item IDs and Opaque values are dropped. Portable
citations remain in IR; each renderer uses only the request representation its
API supports. Provider file IDs cannot cross vendors. Media unsupported by the
target returns `InvalidData`; unsupported optional detail levels/TTLs are omitted.
Responses input images require a detail, so an unmappable level uses native Auto.

## Sources, streams and operations

Sources are Inline bytes, URL, application Reference, or vendor-scoped ProviderFile.
Resolve references explicitly before sending:

```rust,no_run
# use gproxy_client_sdk::*;
# async fn resolve(request: &mut GenerateRequest) -> Result<()> {
request.resolve_assets(|reference| async move {
    // Replace with application blob storage. The caller owns this I/O.
    let bytes = reference.into_bytes();
    Ok(ResolvedAsset::Bytes { bytes, media_type: "text/plain".into() })
}).await?;
# Ok(())
# }
```

The SDK does not read files, fetch arbitrary URLs, upload assets, or transcode.
Asset preparation/Files uploads and Realtime sessions are deferred to later rounds.

`Client::stream()` handles SSE and UTF-8 fragmentation. Events include text,
reasoning, tool arguments, audio, transcripts, citations, complete BlockEnd values,
finish reason/usage and errors. Aggregation uses complete blocks and attaches late
citations. Truncated EOF cannot become Finish. Gemini's separate transcription
parts merge into Audio.transcript while preserving native replay boundaries.

Chat audio parsing needs the requested format because the response does not report
it; `Client` supplies it automatically. Chat streams request usage by default.
For assistant audio history, a valid OpenAI audio ID is reused; after expiry, the
transcript is sent. Missing transcript after expiry returns `InvalidData`.
Gemini speech output supports 24 kHz mono PCM. Responses audio is unsupported;
choose Chat for audio models. Claude and Responses tool results with no function
name acquire it from the earlier matching call ID when rendering to Gemini.

Drop a generation stream or call `cancel()` to release its response body. Transport
cancellation follows `gproxy-client`; observed usage may already be billable.
There are no SDK retries, failover or background polling tasks.

Non-chat methods use `operations::Params<protocol::openai::...>`: typed native
parameters plus the selected vendor's option remainder. Families cover count
tokens, local model listing, embeddings, rerank, TTS/transcription, images and
asynchronous video jobs. Speech/transcription return the native response transport
because it may contain binary, text, JSON or SSE.

## Features and TypeScript

Defaults enable remote, all four dialects and all non-chat operation families.
`embedded` and `ts` are opt-in. Dialect features are `claude`, `openai`, `chat`,
`gemini`; operation features are `count-tokens`, `models`, `embeddings`, `rerank`,
`audio`, `images`, `video`. The `audio` feature controls standalone audio operations;
Chat/Gemini content audio is part of its dialect. Remote normal dependencies do
not include the engine/store/channel/identity crates.

```sh
cargo check -p gproxy-client-sdk --no-default-features --features remote,openai \
  --target wasm32-unknown-unknown
GPROXY_TS_OUT="$PWD/target/client-sdk-types" \
  cargo test -p gproxy-client-sdk --all-features typescript_exports
```

`export_types(directory)` exports the conversation, request/result/events, options
and generic operation parameter graph with workspace ts-rs. Rust's Block alias is
`ContentBlock<Content>` in TS. Complex protocol-native option values export as
JsonValue; portable IR fields retain their concrete TS types. Builds write no
exports unless requested explicitly. Offline tests do not establish live vendor
or Workers runtime behavior.
