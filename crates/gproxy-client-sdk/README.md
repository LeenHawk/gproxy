# gproxy-client-sdk

A caller-facing SDK for programs calling GPROXY over HTTP or through an embedded
`gproxy-sdk::Gproxy`. Its IR stays outside the engine. Native mapping uses existing
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
Use `Client::new(backend::Embedded::new(engine, scope))` on native targets for the
same API. The embedding host owns engine configuration, scope and access policy.

## Content, diagnostics and native mapping

Each `ContentBlock<T>` has vendor-scoped metadata and an optional cache breakpoint.
`Block` aliases `ContentBlock<Content>`. Content distinguishes Text, Reasoning,
Image, Document, File, Audio, Video, ToolCall, ToolResult and Opaque. Text has inline
citations and a portable `refusal` flag. Tool results contain JSON or restricted
Text/Image/Document blocks; custom Document chunks contain Text/Image blocks.
Documents retain title/context, citation settings and detail. PCM Audio requires
sample rate and channels; Video has clip offsets/fps. Model-generated images are
Image blocks. Native server-tool history retains `provider_executed`; the SDK does
not execute tools.

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

Other thin native families cover count tokens, local models, rerank,
TTS/transcription, images and asynchronous video jobs. Speech/transcription retain
the native response transport because it may contain binary, text, JSON or SSE.
Operations are split into feature-scoped modules, with no module-wide dead-code or
unused-import exemption.

Sources remain Inline, URL, application Reference or vendor-scoped ProviderFile.
`GenerateRequest::resolve_assets` and `EmbedRequest::resolve_assets` call the
application resolver (`ResolvedAsset::Bytes { bytes, media_type }` or `Url`). The
SDK does not read local files, fetch arbitrary URLs, upload or transcode. Asset
preparation/uploads and Realtime remain deferred.

## Persistence, features and TypeScript

Persist `Conversation::V1`: `{"schema_version":"1","messages":[...]}`. This round
may change V1 in place before any caller persisted it. Unknown content/nested enum
shapes retain raw JSON; string enums and unified finish categories retain unknown
strings. Serializing Unknown emits the original value, allowing a newer reader to
recover it. Rendering does not interpret unknown content as a known type: it warns
and omits unsupported content/roles/controls, or returns InvalidData for a required
unknown media source/format. Schema versions remain explicit and unknown versions
are rejected. After this round, incompatible shape changes need a new version and
migration; compatible fields need defaults. Unknown enum variants alone do not.

Defaults enable remote, all four renderers and non-chat operation families.
`embedded` and `ts` are opt-in. Dialects: claude/openai/chat/gemini. Operation
families: count-tokens/models/embeddings/rerank/audio/images/video. `audio` controls
standalone audio operations; content audio belongs to its renderer. Normal remote
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
