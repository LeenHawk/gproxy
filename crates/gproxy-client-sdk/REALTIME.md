# Realtime sessions

Enable the opt-in `realtime` feature. It adds OpenAI Realtime GA and Gemini Live
to the existing `Client<B>`, independently of language/media renderer features.
It does not enable an HTTP backend; remote callers also enable `remote`.

`Client::connect_realtime(&RealtimeConfig)` returns `RealtimeSession { sender,
events }`; `split()` returns those two handles. `RealtimeSender` is cloneable.
Poll `RealtimeStream` alongside application input and playback. The SDK creates
no background runtime task. Connection establishment opens WebSocket and sends
the initial configuration; wait for `Ready` before calling sender operations.
Warnings appear before readiness. `Closed` ends the stream.

```rust,no_run
use futures_util::StreamExt;
use gproxy_client_sdk::{*, realtime::*};

async fn session<B: Backend>(client: &Client<B>) -> Result<Conversation> {
    let mut config = RealtimeConfig::new(RealtimeModel {
        id: "provider/model".into(), dialect: RealtimeDialect::Openai,
    });
    config.instructions = Some("Keep replies brief.".into());
    config.input_transcription = Some(RealtimeInputTranscription {
        model: Some("gpt-transcribe".into()), ..Default::default()
    });
    let RealtimeSession { sender, mut events } = client.connect_realtime(&config).await?;
    let mut archive = RealtimeCollector::new();
    while let Some(event) = events.next().await {
        archive.push(&event);
        match event.kind {
            RealtimeEventKind::Ready => sender.send_text("Hello").await?,
            RealtimeEventKind::AudioDelta { bytes, format, .. } => {
                // Pass bytes and the reported format to the application's player.
                let _ = (bytes, format);
            }
            RealtimeEventKind::ClearLocalPlayback => {
                // Immediately discard queued playback in the application's player.
            }
            RealtimeEventKind::ResponseDone { .. } => sender.close(1000, "done").await?,
            RealtimeEventKind::Closed { .. } => break,
            _ => {}
        }
    }
    Ok(archive.into_conversation())
}
```

Configuration includes instructions, voice, input/output `AudioFormat`, output
modalities, `TurnDetection::{ServerVad, SemanticVad, Manual}`, tools, tool choice,
optional input transcription, history and automatic response creation. Model and
dialect are explicit. `Unknown` string/JSON variants survive serialization.
No model-name capability heuristics or audio transcoding are used.

## Native mapping

| Behavior | OpenAI Realtime GA | Gemini Live |
|---|---|---|
| Handshake | GET `/v1/realtime?model=<encoded ID>` | GET `/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent` |
| First frame | `session.update` | `setup`, with `models/<ID>` |
| Ready | First `session.updated` acknowledgment | `setupComplete` |
| Later configuration | `update_session`, then `Updated` acknowledgment | Unsupported; start a new session |
| Input audio | 24 kHz mono PCM, or `audio/pcmu` / `audio/pcma` | 16 kHz mono PCM |
| Output audio | 24 kHz mono PCM, or PCMU / PCMA; acknowledged native format is observed | 24 kHz mono PCM |
| Turn detection | Native server VAD, semantic VAD, or null for manual | Automatic activity detection; semantic VAD warns and maps to automatic |
| Tool selection | Native auto/none/required/named function | No setup tool-choice field; warns |
| Tool-result continuation | Optional `response.create`, enabled by default | Native tool response resumes generation; disabling automatic continuation warns |
| History | `conversation.item.create` after readiness | `clientContent` after readiness, with `turnComplete=false`; system text goes in setup |

PCM uses 16-bit samples. Formats outside the supported combinations return
`InvalidData`. No resampling, channel mixing, encoding or decoding is performed.
Audio output includes transcripts on OpenAI. Gemini setup requests output audio
transcription for persistence. Enable `input_transcription` to archive user speech;
unreported transcripts cannot be reconstructed by the collector.

Per-vendor options use existing protocol types: OpenAI's typed session patch and
Gemini's Live generation, activity, transcription, tools, resumption, compression,
safety, proactivity, history and avatar settings, with `rest` for extensions.
Native fields override portable settings with compatibility warnings. Partial
OpenAI audio patches preserve unrelated portable fields; Gemini speech language
settings preserve a portable voice unless native voice controls override it.
Options for the other vendor are omitted with a warning. Unknown/unsupported
settings warn; inputs without a native representation return errors.

History preparation supports text and tools, including archived speech transcripts.
Typed Interrupted records are silently skipped when seeding a session. Other media
history must be adapted by the caller before connecting. Previous-session cache
breakpoints and native continuation metadata are not attached to a new session's
items. The original history remains unchanged. The collector archives new events;
the application retains and appends its original history.

## Sending and interruption

The sender exposes `append_audio`, `commit`, `clear`, `send_text`,
`send_tool_result`, `send_tool_result_with_response`, `create_response`, `cancel`,
`interrupt(played_ms)`, `update_session`, `send_native` and `close(code, reason)`.
`auto_response` defaults to true for portable text/tool-result sends. OpenAI
manual audio commit does not create a response; call `create_response` explicitly.
Gemini manual input brackets appended audio with activityStart/activityEnd;
automatic input uses audioStreamEnd on commit. Live has no input-clear or
response-cancel operation, so those methods return `Unsupported`.

OpenAI interruption cancels the active response owning that audio item, then sends
`conversation.item.truncate` for the observed audio item/content index. Completed
generation only needs truncate. The SDK emits `ClearLocalPlayback` and
`Interrupted`, suppresses late audio for that item and retains the server's
truncate acknowledgment. It never sends `output_audio_buffer.clear`, which is
WebRTC/SIP-only. On VAD speech-start, the application can call interrupt with its
player's actual played milliseconds; the SDK does not estimate playback time.

Gemini interrupt clears local playback and suppresses remaining local output;
it cannot truncate remote context. Native `serverContent.interrupted` also emits
the interruption events. Native interruption does not report played milliseconds,
so the marker keeps that value unknown unless the application supplied it.

`send_native(NativeRealtimeMessage::Openai(&event))`, its Gemini counterpart and
`Raw(&value)` borrow native messages without copying their DTO trees. Typed
messages must match the session dialect; Live setup cannot be sent a second time.
Native/raw sends and arbitrary `rest` extensions are escape hatches: callers own
the state and persistence effects of operations outside the portable API.

## Events and persistence

Events cover readiness/updates, VAD, input commit/clear, input transcription
delta/completion/preview/failure, reply start/completion, audio, output transcripts,
text, tool calls/cancellation/results, interruption/local playback clear, rate
limits, goAway, resumption, warnings, errors and close codes. Every event has vendor
metadata. Native events retain the original JSON; local/control events have no
native JSON. OpenAI dispatches all known server event tags into concrete protocol
DTOs before mapping, avoiding the untagged enum's trial order. Gemini decodes
message fields together, including messages carrying several fields. Unmapped
events remain raw `Unknown` values. Gemini item IDs are local correlation IDs;
missing native response IDs and usage counters remain unknown.

`RealtimeCollector::push(&event)` and `into_conversation()` retain the existing
`Conversation::V1` format. Audio bytes and raw native envelopes never enter the
collector. User/model speech becomes Text with `realtime_speech_transcript`
metadata. Plain text remains Text; tools become ToolCall/ToolResult. Failed input
transcription removes provisional text. Partial input transcripts close at a new
VAD boundary if no native finished flag arrived; Gemini does not provide native
item IDs for correlating input transcription with other independently sent fields.

Interrupted speech loses its entire transcript, including previously completed
text and late deltas. `Content::Interrupted(Interruption { played_ms })` records
the interruption; the variant itself implies an Interrupted finish reason. It
carries no vendor metadata and all model renderers silently skip it. Repeated native
acknowledgments preserve an already reported playback duration. Native cancellation
and an unfinished connection close also remove provisional model speech. Plain
text/tool blocks retain their finish reason. No partial transcript is guessed
from audio duration, arrival time or character count.

Handshake rejection preserves upstream HTTP status/body. In-band OpenAI errors
and input transcription failures are recoverable; malformed frames, transport
failures, abnormal closes and Gemini RPC errors are fatal. In-band errors do not
invent HTTP status codes. No retries, reconnects, resumption scheduler, failover,
tool execution or application playback are implemented.

## Transports, scope and verification

Remote's normal constructor obtains a separate HTTP/1.1 WebSocket client through
`ClientPool::get_websocket`. Connect uses `OutboundClient::connect`, converts the
deployment's http(s) origin to ws(s), preserves mount prefixes, and supplies the
gateway Bearer header. `with_transport` also allows a caller-owned duplex transport.
With remote + realtime on wasm, `gproxy-client/workers` uses Fetch with Upgrade and
accepts the returned Cloudflare `response.webSocket`. It does not use the browser
WebSocket constructor. Browser direct connection, WebRTC and SIP are outside scope.

`gproxy_sdk::ClientBackend` passes the model explicitly to `Gproxy::connect`, with host scope and
cancellation. It retains the engine's usage completion for the connection lifetime.
The Backend trait supplies a default Unsupported connection method, preserving
existing HTTP-only implementations, and boxed backends forward it.

Gateway model admission/alias rewriting from handshake/setup, and aistudio/Gemini
ConnectRealtime channel support, remain separate gateway work. Protocol, gateway
and other crates are unchanged by this SDK extension.

Offline verification uses saved scripted native frame fixtures, transport injection,
collector serde round trips and memory/custom embedded routing. Fixture audio and
IDs are synthetic; no paid upstream recording or smoke test is claimed. Workers
validation is a wasm build, not deployment/runtime certification. Test logs stay
under `target/client-sdk-realtime/`.
