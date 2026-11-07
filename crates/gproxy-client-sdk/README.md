# gproxy-client-sdk

Review the module boundaries, test groups and per-file line counts in [LAYOUT.md](LAYOUT.md).

A caller-facing SDK for programs calling GPROXY over HTTP, or an in-process
`gproxy_sdk::Gproxy` through `gproxy_sdk::ClientBackend`. Its IR stays outside the
engine, and this crate does not depend on it. Native mapping uses existing
protocol structs/enums/builders; HTTP/SSE reuse `gproxy-client`, including wasm fetch.

```rust,no_run
use gproxy_client_sdk::{backend::Remote, *};

# async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
let client = Client::new(Remote::new("https://gproxy.example", "gateway-key").await?);
let mut request = GenerateRequest::new(
    Model { id: "provider/model".into(), dialect: Dialect::Openai },
    vec![Message { role: Role::User, content: vec![Block::text("Hello")] }],
);
request.settings.reasoning = Some(ReasoningSettings {
    effort: ReasoningEffort::High, token_budget: None,
});
let result = client.generate(&request).await?;
// These are provider-reported facts; no requested model or receipt time is substituted.
let served_model = &result.response.model;
let generated_at = result.response.timestamp; // Unix seconds, when reported
let finish = &result.finish_reason; // unified category + raw native value
let warnings = &result.warnings;
if let Some(id) = &result.response.id {
    request.provider_options.openai.responses.previous_response_id = Some(Some(id.clone()));
}
let history = Conversation::new(vec![result.into_message()]);
let persisted = serde_json::to_string(&history)?;
# Ok(())
# }
```

`Model.dialect` is explicit: Claude Messages, OpenAI Responses (`Openai`), OpenAI
Chat Completions (`OpenaiChat`), or Gemini GenerateContent. The local model catalog
and permissions are unchanged; `owned_by` is not a dialect guarantee. Claude
requires `settings.max_output_tokens` or `provider_options.claude.max_tokens`.
For the same API in process, enable `gproxy-sdk`'s `client-sdk` feature and use
`Client::new(gproxy_sdk::ClientBackend::new(engine, scope))` on native targets. The embedding host owns engine configuration, scope and access policy.

## Content, diagnostics and native mapping

Each `ContentBlock<T>` has vendor-scoped metadata and an optional cache breakpoint.
`Block` aliases `ContentBlock<Content>`. Content distinguishes Text, Reasoning,
Image, Document, File, Audio, Video, ToolCall, ToolResult, Interrupted and Opaque. Text has inline
citations and a portable `refusal` flag. Tool results contain JSON or restricted
Text/Image/Document blocks; custom Document chunks contain Text/Image blocks.
Documents retain title/context, citation settings and detail. PCM Audio requires
sample rate and channels; Video has clip offsets/fps. Model-generated images are
Image blocks. Native server-tool history retains `provider_executed`; the SDK does
not execute tools.

`Interrupted(Interruption { played_ms })` records interrupted speech and implies an
Interrupted finish reason. It is silently excluded from model input by every
renderer and from Realtime history seeding, including its metadata and cache fields.

SDK-owned metadata preserves signatures, encrypted/redacted reasoning, native item
IDs/status, citation anchors, audio ID/expiry, layout boundaries and unknown Rest.
Portable text/media/tool payloads stay in IR. Only Opaque retains a complete native
value. No WireOptions or modeled-wire JSON overlays are used.

`ProviderOptions` contains typed `claude`, `openai.responses`, `openai.chat` and
`gemini` options, plus native extension remainders. Explicit vendor controls take
precedence over their portable counterparts and produce compatibility diagnostics
when a portable control is ignored. Vendor tools supplement portable tools. Use
Rest for extensions, without duplicating modeled request fields.

Typed `dialects::{claude,openai,chat,gemini}::render` methods return
`Rendered<NativeRequest> { request, warnings }`; `dialects::render` returns the same
wrapper with a JSON request. `Client` sends only the native request. Results carry
`warnings: Vec<Warning>` with unsupported/compatibility/deprecated/other kinds,
a concerned field/path and a message. Warnings cover SDK omissions, detail/TTL
fallbacks, dropped vendor/dialect metadata, ignored document controls and native
option precedence. Model-specific support is left to the upstream API; the SDK
uses no model-name capability heuristics.

Refusal remains an IR property when switching vendor. Chat encodes its native
assistant refusal field. Responses can replay native refusal items with their
message envelope. Claude/Gemini and Responses without that envelope send the text
and warn when they cannot carry a native refusal marker. The flag stays in the
original IR. Retrieved `sources` are separate from inline citations: grounding
chunks and search results do not need inline anchors. An unlocated snippet keeps
its title/text with `source: None`; no URL or file identity is invented.

When a vendor reference declares an optional field nullable, null and omitted are
equivalent. Renderers may omit null when protocol cannot emit it. Required nullable
Claude citation titles/count context stay null using the supplied protocol types.
Other vendors' signatures, encrypted/redacted state, native IDs and Opaque data
are not replayed; omissions are reported. Provider file IDs cannot cross vendors.
Unsupported media/source formats return `InvalidData`. Unmappable optional detail
or TTL is omitted with a warning; Responses images use Auto where required.

## Reasoning

| Dialect | Portable reasoning mapping |
|---|---|
| Claude | None disables thinking; levels use adaptive thinking + effort; Minimal maps to Low with warning |
| Claude budget | Explicit budget selects enabled/budgeted thinking; it takes precedence over portable adaptive effort with warning |
| Responses / Chat | None through Max map to native effort; token budget is unsupported and warned |
| Gemini | None uses budget 0; Minimal/Low/Medium/High use thinking level; Xhigh/Max map to High with warning |
| Gemini budget | Explicit budget takes precedence over portable thinking level with warning |

ProviderDefault adds no control. Explicit native reasoning options override
portable controls. Native field/model constraints remain upstream constraints;
Gemini thinking levels target models supporting that API, and vendor options can
select a native budget/configuration instead. No retry or fallback model is chosen.

## Streams and errors

`Client::stream()` handles SSE/UTF-8 fragmentation. `Event::Start` delivers known
warnings before content; later provider-derived diagnostics use `Event::Warning`.
ResponseMetadata and Source events are emitted when observed. Content events cover
text, reasoning, tool arguments, audio, transcripts, citations and final BlockEnd
values. Aggregation attaches late citations and collects response metadata/sources.
Claude text BlockEnd waits for the native refusal decision. Gemini separate
transcription parts merge into Audio.transcript while keeping native replay layout.

`FinishReason { unified, raw }` includes Stop, Length, ToolCalls, ContentBlocked,
Error, Interrupted and Other. Responses uses its native status/incomplete reason;
other dialects use native stop/finish values. A clean EOF without completion gives
Interrupted with observed partial content, not Stop. An in-band error emits Error
then Finish(Error); malformed records or transport faults finish Interrupted.
Aggregation returns the error when an Error event is present. Cancellation consumes
and drops the stream; it does not fabricate a completion event. Transport
cancellation follows gproxy-client and observed usage may already be billable.

`Error` has a small `ErrorKind` enum, optional observed HTTP status, retryable,
original body when available, original Retry-After and a message. Kinds include
Auth, PaymentRequired, RateLimit, Overloaded, Timeout, NotFound, ContextTooLong,
ContentBlocked, Upstream, Network and InvalidData. Retryability covers
408/409/429/5xx and native overload/transient signals. Typed transport timeouts are
classified without guessing from message text. A standalone decoder has no HTTP
status; Client attaches the actual head, including 200/201 for in-band errors.
For SSE errors, body is the original event data. There are no automatic retries.

Chat audio parsing needs the requested format; Client supplies the effective native
format automatically. Streams request usage by default. A valid assistant audio
ID is reused; expired/foreign audio falls back to transcript with a compatibility
warning. Missing transcript returns InvalidData. Gemini speech output is 24 kHz
mono PCM; Responses conversation audio is unsupported. Native token counters are
not estimated when unreported.

## Embeddings and other operations

The `embeddings` feature exposes `Client::embed(&EmbedRequest)` with an independent
explicit `EmbeddingModel { id, dialect: EmbeddingDialect::{Openai, Gemini} }`.
It does not require any language-model renderer feature. Inputs are Text, Image or
multipart Text/Image; images/multipart inputs use Gemini. Dimensions and batch size
are positive types. Purposes include Query, Document, Similarity, Classification,
Clustering, QuestionAnswering, FactVerification and CodeQuery, mapped to Gemini
task_type. Standard OpenAI has no purpose parameter and returns an unsupported
warning. Explicit native embedding configuration overrides portable controls.

OpenAI packs at most 2048 inputs per call, restores input order from native indices,
and decodes float/base64 vectors. Gemini uses an ordered embedContent call per input
so the model remains URL-routed; bulk inputs receive a batching compatibility
warning. Usage is summed only when every call reports it. On failure, later calls
are not submitted; completed calls are not retried or rolled back. There is no
concurrency, retry or failover scheduler. The native `Client::embeddings` passthrough
with `operations::Params<protocol::openai::embeddings::...>` remains available.

`list_models(dialect)` and `get_model(&model)` read the gateway's local catalog in
the caller's chosen dialect (OpenAI `/v1/models`, Claude the same path with
`anthropic-version`, Gemini `/v1beta/models`) and return `ModelInfo` entries whose
`model` is ready to use in that dialect at the same endpoint; Gemini's `models/`
prefix is removed. The dialect is the caller's choice, not a property of a model.

Other thin native families cover count tokens, rerank,
TTS/transcription, images and asynchronous video jobs. Speech/transcription retain
the native response transport because it may contain binary, text, JSON or SSE.
Operations are split into feature-scoped modules, with no module-wide dead-code or
unused-import exemption.

## Request-scoped asset preparation

`GenerateRequest::resolve_assets` and `EmbedRequest::resolve_assets` now borrow the
original request and return a send-only snapshot. They accept an async
`FnMut(ResolveAsset) -> Future<Output = Result<ResolvedAsset>>`, with no Send bound.
The input carries the application reference, explicit target dialect, media kind,
content location, known MIME, accepted methods/constraints and preferred method.
The resolver returns `Bytes { bytes, media_type }` or
`Url { url, media_type, byte_len }`. URL MIME/size metadata is optional; it can come
from the caller's object store. The SDK never fetches the URL to discover it.

```rust,no_run
use gproxy_client_sdk::*;
use std::future::Future;
async fn send<B, F, Fut>(client: &Client<B>, request: &GenerateRequest, resolve: F)
    -> Result<GenerateResult>
where
    B: Backend,
    F: FnMut(ResolveAsset) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    let prepared = request.resolve_assets(DeliveryMode::Inline, resolve).await?;
    client.generate_prepared(prepared).await
}
```

`Client::generate_prepared`, `stream_prepared`, `embed_prepared` and the language
snapshot's `render` consume the snapshot. Snapshots have no Serialize, Clone or
Deref implementation. Prepare again for every send, obtaining fresh presigned URLs;
persist the original `Conversation` with application references. The persisted IR
shape is unchanged. Resolver URLs and bytes never replace those references in the
original request. Preparation warnings reach results and the first stream Start.

The public `media_capability(AssetDialect, MediaKind)` table remains available even
with all renderer features disabled. Its message-part delivery support is:

| Dialect | Image | Document | Audio | Video | File |
|---|---|---|---|---|---|
| Claude Messages | Inline / URL | Inline PDF/text; URL PDF | — | — | — |
| OpenAI Responses | Inline / URL | Inline / URL | — | — | Inline / URL |
| OpenAI Chat | Inline / URL | Inline | Inline WAV/MP3 | — | Inline |
| Gemini GenerateContent | Inline / URL | Inline / URL | Inline / URL | Inline / URL | Inline / URL |

Gemini tool-result media accepts inline only; Chat tool messages have no media
representation. Claude custom document chunks recurse through their Image blocks.
Chat assistant audio reuses a native audio ID or transcript; it does not send its
source as input audio. Embedding preparation is independent of renderer features:
Gemini accepts images via both methods; standard OpenAI embeddings accept text only.

Sources for delivery and formats are
[Claude Messages](<../../upstream_docs/claude/docs/Create a Message.md>),
[Responses](<../../upstream_docs/openai/docs/Create a model response.md>),
[Chat](<../../upstream_docs/openai/docs/Create chat completion.md>), and
[Gemini content](<../../upstream_docs/gemini/docs/Generating content.md>), matched to
protocol's typed sources/parts. Claude images accept JPEG/PNG/GIF/WebP. Gemini's
video list follows [Video understanding](<../../upstream_docs/gemini/docs/Video understanding.md>).
Gemini MIME examples are marked non-exhaustive, rather than converted into a whitelist.

Documented byte constraints, with exact source lines in the table implementation:

- Claude: 32 MB for the entire serialized native request, including base64 and text
  ([API overview, line 111](<../../upstream_docs/claude/docs/API overview.md>)).
- Gemini: 100 MB per file input, including external URLs when the resolver reports
  object size ([Changelog, line 522](../../upstream_docs/gemini/docs/Changelog.md));
  inline video is strictly less than 100 MB (Video understanding, line 23).
- The local OpenAI endpoint references and Claude image schemas do not specify
  additional per-asset byte caps. Their table entries remain unknown. MB uses SI bytes.

`resolve_assets` takes a `DeliveryMode`. `Inline` (the default) asks for inline
bytes and needs no object store; `PreferUrl` asks for a URL when the content
envelope and known format accept one. Either mode falls back to the other method
when the target accepts only that one. A resolver returning an allowed method
other than the preferred one produces a compatibility warning. The built-in S3
resolver follows the same preference and only touches the store for URLs;
`resolve_assets_s3` uses `PreferUrl`. Unsupported methods, known format
violations and exceeded documented byte limits return InvalidData. Resolver errors
are returned without retry. Direct caller URLs/bytes are checked and used as-is,
without calling the resolver or producing a preference warning. Resolver bytes
always carry a MIME type; native replay may omit it, and renderers that need one
return InvalidData. No model-name capability heuristics or transcoding are added.

Generic preparation needs no storage credentials. The optional built-in `s3`
resolver accepts explicit credentials as described below. Preparation performs no
local-file reads, media URL fetches, vendor Files API uploads, registration or
upload-ID caching. Existing native ProviderFile
IR remains for same-vendor history replay; asset preparation rejects those IDs and
offers only inline/URL.

## Built-in S3 resolver (`s3` feature)

Enable `s3` to use `s3::{S3AssetConfig, S3AssetResolver, AssetData}`. This adds
optional workspace dependencies on `gproxy-file` with its S3 service and `sha2`;
it uses the exported OpenDAL Operator for stat/write/presign. No second S3 client
or SDK-owned signing implementation is used. Defaults do not include this feature.

Configure endpoint, bucket, region and access/secret keys with
`S3AssetConfig::new(...)`; optional session_token, key_prefix, presign_expiry and
virtual_host_style are public fields. The default expiry is five minutes, prefix
is empty and addressing is path-style. Region `auto` works for R2; MinIO can use
an HTTP endpoint with a port. Endpoint/addressing/signature behavior comes from
OpenDAL. Credential file/environment/EC2 discovery is disabled. Credentials are
held only by the configured store; config/store/client are not serializable.

```rust,no_run
use gproxy_client_sdk::{*, s3::{AssetData, S3AssetConfig, S3AssetResolver}};
use std::future::Future;
async fn send<B, F, Fut>(client: Client<B>, request: &GenerateRequest,
    access_key: &str, secret_key: &str, load: F) -> Result<GenerateResult>
where
    B: Backend,
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<AssetData>>,
{
    let mut config = S3AssetConfig::new(
        "https://account.r2.cloudflarestorage.com", "assets", "auto",
        access_key, secret_key,
    );
    config.key_prefix = "conversation-assets".into();
    let client = client.with_s3(S3AssetResolver::new(config)?);
    client.generate(request, load).await
}
```

Keep the configured client and reuse it. `S3Client::generate`, `stream` and `embed`
accept only an async `reference -> AssetData { bytes, media_type }` callback, without
a Send bound. Other native operations remain available through `S3Client.client`.
For explicit preparation, call `request.resolve_assets_s3(&store, load)`. A host can
inject an Operator via `S3AssetResolver::from_operator`, including an Operator made
with `gproxy_file::s3_with_transport(config.builder(), transport)`.

Each reference is loaded on each request. The resolver checks known MIME/size
constraints before storage I/O. If URL is accepted for the resolved MIME, the key
is `<prefix>/<lowercase SHA-256(bytes)>.<extension>`, for example `.png`, `.pdf` or
`.mp3`; aliases use canonical extensions and unknown types use `.bin`. The reference
is never interpreted as a path. `stat` skips an existing object; NotFound triggers
`write_with(...).content_type(...)`. Every URL result calls `presign_read` afresh;
URLs are not cached or copied into history. Presigns in the same second can have
the same URL even though both signing operations ran.

Inline-only media returns bytes without stat/write/presign. When a resolved MIME
narrows the allowed methods (for example a Claude plain-text document), the ordinary
round-3 fallback warning still applies. Storage failures propagate without SDK
retry or silent inline fallback. Injected operators requiring custom GET headers
cannot be represented by a vendor URL source and return InvalidData.

Deduplication is stat-then-write, not a distributed lock: concurrent first writers
may upload the same content key. Objects are not deleted by the SDK; callers own
bucket retention, lifecycle and access policy. Existing objects are trusted to
match their content keys and retain their original Content-Type. Unit tests use a
fake HTTP service with the real OpenDAL S3 signer; no live R2/MinIO or Workers
runtime behavior is claimed.

## Portable media operations

Independent image, speech and transcription IRs are available alongside native
passthroughs. See [MEDIA.md](MEDIA.md) for APIs, direct Gemini mappings, diagnostics,
streaming and asset preparation. Video remains a native passthrough.

## Persistence, features and TypeScript

The opt-in `realtime` feature adds `Client::connect_realtime`, a cloneable sender,
an event stream and a transcript-only Conversation collector for OpenAI Realtime
GA and Gemini Live. Native and Cloudflare Workers remote WebSockets and
`gproxy_sdk::ClientBackend` share the API. See [REALTIME.md](REALTIME.md) for setup, interruption,
typed options, transport boundaries and offline verification.

Persist `Conversation::V1`: `{"schema_version":"1","messages":[...]}`. This round
may change V1 in place before any caller persisted it. Unknown content/nested enum
shapes retain raw JSON; string enums and unified finish categories retain unknown
strings. Serializing Unknown emits the original value, allowing a newer reader to
recover it. Rendering does not interpret unknown content as a known type: it warns
and omits unsupported content/roles/controls, or returns InvalidData for a required
unknown media source/format. Schema versions remain explicit and unknown versions
are rejected. After this round, incompatible shape changes need a new version and
migration; compatible fields need defaults. Adding Content variants such as
Interrupted keeps V1: older readers preserve their raw value through Unknown.

Defaults enable remote, all four renderers and non-chat operation families.
`ts`, `s3` and `realtime` are opt-in. Dialects: claude/openai/chat/gemini. Operation
families: count-tokens/models/embeddings/rerank/audio/images/speech/transcription/video.
`audio` controls native audio passthroughs; speech/transcription have independent
IR features, and content audio belongs to its renderer. Normal remote
dependencies exclude the engine/store/channel/identity crates.

```sh
cargo check -p gproxy-client-sdk --no-default-features --features remote,openai \
  --target wasm32-unknown-unknown
cargo check -p gproxy-client-sdk --no-default-features --features remote,embeddings \
  --target wasm32-unknown-unknown
GPROXY_TS_OUT="$PWD/target/client-sdk-types" \
  cargo test -p gproxy-client-sdk --all-features typescript_exports
```

`export_types(directory)` exports the IR/results/events/options/embedding graph
with workspace ts-rs. Block is `ContentBlock<Content>` in TS. Native complex options
export as JsonValue. Raw Unknown alternatives broaden unions; TS readers must
check unfamiliar values before accessing known fields. Builds write exports only
when requested; scratch stays under target/ or /tmp. Offline verification does not
establish live provider or Workers runtime behavior.

Design references only: [Vercel provider v4](https://github.com/vercel/ai/tree/main/packages/provider/src/language-model/v4)
(`@ai-sdk/provider` 4.0.35) and [OpenRouter SDK errors](https://github.com/OpenRouterTeam/typescript-sdk/tree/main/src/models/errors).
No dependencies or implementation code are imported from them.
