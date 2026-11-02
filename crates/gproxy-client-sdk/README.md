# gproxy-client-sdk

The caller-facing Rust SDK for programs calling a GPROXY deployment or an
embedded `gproxy-sdk::Gproxy`. It uses `gproxy-client` for HTTP, including fetch
on `wasm32-unknown-unknown`. It does not run retries or failover.

```rust,no_run
use gproxy_client_sdk::{backend::Remote, *};

# async fn example() -> Result<()> {
let client = Client::new(Remote::new("https://gproxy.example", "gateway-key").await?);
let request = GenerateRequest::new(
    Model { id: "provider/model".into(), dialect: Dialect::Openai },
    vec![Message { role: Role::User, content: vec![Block::text("Hello")] }],
);
let result = client.stream(&request).await?.aggregate().await?;
let history = Conversation::new(vec![result.into_message()]);
let persisted = serde_json::to_string(&history).unwrap();
# Ok(())
# }
```

For Claude, supply `settings.max_output_tokens` (or native `max_tokens` in
`provider_options[Vendor::Claude]`); the native API requires it. For stateless
OpenAI reasoning, request `include: ["reasoning.encrypted_content"]` in OpenAI
provider options. The SDK preserves the encrypted items it receives.

Use `Client::new(backend::Embedded::new(engine, scope))` on native targets for
the same generation/operation methods. The embedding host configures the engine
and owns access policy. Remote requests use the gateway key's policy.

The model's dialect is explicit. `list_models()` returns GPROXY's normal local
catalog, whose `owned_by` field is not a dialect guarantee. Set `Model.dialect`
from application configuration; no model-name heuristics are used.

`dialects::{claude,openai,chat,gemini}::{render,parse}` expose the native wire-type
boundaries. `stream::StreamDecoder` accepts native JSON events when a host
already owns framing. `Client::stream()` handles SSE and UTF-8 fragmentation.
Drop a stream or call `cancel()` to release the request. Billing follows
GPROXY's observed usage and cancellation semantics; cancellation does not undo
tokens already generated upstream.

`Asset` supports inline bytes, URLs, provider file IDs and application blob
references. Use `client.prepare_assets(&mut request, resolver).await?` before
sending. The resolver returns `assets::ResolvedAsset::Bytes { bytes, media_type }`
or `Url { url }`. The SDK chooses inline data, a supported URL, or a Files upload,
using protocol's existing multipart/Gemini resumable capabilities. It never reads
local files, fetches arbitrary URLs, or transcodes media. Unsupported formats and
size limits return `InvalidData`.

Preparation keeps portable bytes and stores reusable file IDs under
`provider_metadata[vendor].upload`; expired IDs are uploaded again. The default
inline threshold is 1 MiB; `prepare_assets_with` can change it. Gemini processing
IDs are saved before returning an error; call preparation again when the caller
chooses to refresh. There is no polling or retry task. Vendor file IDs never
cross vendors. An asset with only a foreign file ID needs portable material from
the caller. Files uploads include an `x-gproxy-model` routing hint consumed by the
gateway, so the upload follows the same model/provider plan as generation.

Persist `Conversation`, whose v1 JSON envelope is
`{"schema_version":"1","messages":[...]}`. Unknown versions fail to decode.
The rich content revision changes V1 in place before any caller has persisted
it, as authorized for this round. From this point, changes must be additive with
serde defaults; a breaking representation needs a new version and migration. Keep `provider_metadata` with the
content: it contains signatures, native item IDs, annotations, encrypted
reasoning, and unknown vendor extensions. Portable text, tool arguments and
media payloads live in the IR rather than being duplicated in metadata.
SDK-owned metadata keys are `native`,
`item`, `group`, `cache`, `upload`, and `chat`. Native metadata stays authoritative for opaque blocks;
edits to portable text/tool fields are applied when rendering.

Rendering constructs protocol structs/builders directly, and parsing matches
native enum variants. Only provider options, persisted metadata, schemas and
schema-free tool payloads cross a JSON boundary. Protocol `WireOptions`
projections keep these boundaries typed without relaxing required fields on
complete native requests. Unknown fields stay in the wire types' `rest` bags.
Responses reasoning metadata retains its native summary segmentation because
the IR has a single reasoning string.

Claude and OpenAI tool results can omit the function name. Gemini rendering
resolves an empty name from the matching tool-call ID earlier in the request;
an unresolved result returns `InvalidData`. Native result import helpers are
`claude::parse_tool_result` and `openai::parse_tool_result`.

When changing vendors, only that vendor's metadata/options are read. Signed
reasoning, redacted/encrypted reasoning, item IDs, native citation extensions,
unknown native blocks, and provider options from other vendors are dropped.
Portable citation sources/title/ranges remain in the IR; renderers use them only
where the target API has a request representation. Unsigned reasoning
is omitted rather than manufactured into signed native history. Portable text,
images, documents, audio/video and function results are rendered when supported
by the target. Claude accepts images/documents; Responses accepts images/files;
Chat accepts input WAV/MP3 and audio-ID assistant history; Gemini supports all
media kinds. Custom document chunks are Claude-only. Unsupported kinds return an
error. Unmappable detail levels and cache TTLs are omitted.
OpenAI Responses has no stop-sequence field; requesting it returns `InvalidData`.
Gemini generation returns candidate zero, with streamed parts kept in native
order (including separate text parts); request one candidate for this API.

Content blocks distinguish `Text` (with citations), `Reasoning`, `Image`,
`Document`, `File`, `Audio`, `Video`, `ToolCall`, `ToolResult`, and `Opaque`.
Every block has vendor metadata and an optional cache breakpoint. `Document`
supports PDF/text/assets/custom chunks, title/context/citation settings and detail;
`AudioFormat::Pcm` requires sample rate and channels. Tool results are JSON or
restricted text/image/document blocks. `provider_executed` distinguishes native
server tools from tools the application should execute. Model output images are
portable `Image` blocks too.

Output modalities and speech voice/format live in `Settings`. Streams emit audio,
transcript and citation events alongside text/reasoning/tools. `BlockEnd` holds
the complete block; aggregation also attaches late citations. Gemini audio and
separate transcription parts merge into `Audio.transcript`, retaining native
boundaries for replay. Chat parsing requires the requested audio format (the
response does not carry it); `Client` passes it automatically. `chat::render_at`
allows deterministic expiry handling: a valid native audio ID is reused, while
expired audio becomes its transcript. Missing transcript after expiry is an error.

With `realtime`, `client.connect_realtime(OpenaiRealtimeOptions::new(model))`
returns a session with `send(Command)`, `receive()`, and `conversation()`.
`split()` permits sending microphone chunks while waiting for events. Native
session options remain typed protocol DTOs. `create_realtime_call(options, sdp)`
returns the SDP answer and Location; the host owns its WebRTC peer, media tracks,
and data channel. `OpenaiSessionCodec` runs on that channel, with
`record_output_audio` accepting audio decoded from a host media track. Gemini
Live is deferred. These APIs were tested with local transports/typed fixtures;
no live vendor audio session or browser WebRTC negotiation was performed.

Features (`embedded`, `realtime`, and `ts` are opt-in):

| Feature | API |
|---|---|
| `remote` | `backend::Remote`; native reqwest or wasm fetch |
| `embedded` | Native `backend::Embedded<C>`; optional engine dependencies |
| `claude`, `openai`, `chat`, `gemini` | Native renderers and stream decoders |
| `asset-upload` | Files uploads and reusable vendor IDs during preparation |
| `realtime` | OpenAI WS session, WebRTC SDP setup and data-channel codec |
| `count-tokens`, `models` | Native token-count params, local model catalog |
| `embeddings`, `rerank` | Native typed JSON params/results |
| `audio` | Typed speech params; multipart transcription params |
| `images` | Typed generation/edit JSON params/results (URL/data URL/file ID) |
| `video` | Async create/retrieve/list/delete/download; no polling loop |
| `ts` | `export_types(directory)` using workspace ts-rs |

Non-chat JSON methods accept `operations::Params<protocol::openai::...>`:
typed native parameters, the target `Vendor`, and `provider_options`. GPROXY's
public OpenAI-shaped media/embedding endpoints adapt these upstream. Parameters
override options of the same name. Token counting uses its selected native
dialect. Speech and transcription return `WireResponse<HttpBody>` because the
requested result can be binary, text, JSON or native SSE. Read or stream it
with `protocol::codec`; dropping it cancels consumption too.

```sh
cargo check -p gproxy-client-sdk --no-default-features --features remote,openai \
  --target wasm32-unknown-unknown
GPROXY_TS_OUT="$PWD/target/client-sdk-types" \
  cargo test -p gproxy-client-sdk --features ts typescript_exports
```

TypeScript exports cover the versioned conversation, request/result/event IR,
generic operation parameters, token counts and transcription parameters.
Rust's `Block` alias is represented as `ContentBlock<Content>` in TypeScript;
restricted tool/document blocks use the same generic with their own content enum. Native
non-chat parameter/result types remain in `gproxy-protocol`; the generic TS
`Params<T>` accepts the application's native operation shape. Normal builds and
tests do not write declarations unless `GPROXY_TS_OUT` is supplied.
