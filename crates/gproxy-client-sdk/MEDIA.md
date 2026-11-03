# Image, speech and transcription APIs

These operation families have independent features and explicit models:
`images::{ImageModel, ImageDialect}`, `speech::{SpeechModel, SpeechDialect}` and
`transcription::{TranscriptionModel, TranscriptionDialect}`. Each dialect enum has
Openai, Gemini and a raw Unknown fallback. Claude/unknown dialects return InvalidData.
No model-name capability heuristics, retries, failover, transcoding, arbitrary URL
fetches or local-file reads are added. Video remains a native passthrough.

All three families are enabled by default. `images` also retains the existing image
passthroughs. The independent `speech` and `transcription` features do not enable a
language renderer or the old `audio` passthrough feature. Each builds alone on wasm.

## Images

`Client::generate_images(&ImageRequest)` handles generation and editing through
one interface. Its inputs are prompt, Image blocks, an optional mask, positive n,
optional AspectRatio, ResolutionTier (512/1K/2K/4K), exact ImageSize, seed and PNG/
JPEG/WebP output format. `stream_images` returns ImageEvent Start, PartialImage,
Finish(ImageResult), Error or a persisted Unknown value.

- OpenAI uses native CreateImage when input images are empty and EditImage JSON
  otherwise. Edit references and masks can be URLs or base64 data URLs. Masks need
  an edit input and must use PNG when their MIME is known. The documented URI
  length limit is checked after encoding. Seed is omitted with a warning.
- Exact pixel size overrides ratio/tier and is passed unchanged to OpenAI. For
  ratio/tier mapping, the tier is the long edge and the other edge follows the
  ratio, rounded to a multiple of 16 with a warning if inexact. Aspect-only OpenAI
  requests use a documented SDK default of a 1K long edge and warn. Model-specific
  size constraints remain upstream constraints.
- Gemini goes directly to GenerateContent with imageConfig and TEXT/IMAGE output
  modalities. Ratios use the nearest ratio in the checked-in native reference;
  exact pixel sizes are approximated by ratio and nearest supported tier, with
  warnings. Native generationConfig controls take precedence. The native
  response_format image MIME enum documents JPEG only, so PNG/WebP requests are
  omitted with a warning. Masks return an error. n > 1 runs n ordered calls,
  without retrying a failed call or rolling back completed work.
- OpenAI quality, background, compression, moderation and input fidelity are typed
  vendor controls. Gemini generation config and safety settings use existing
  protocol types plus Rest. Vendor controls from the other dialect are not sent
  and produce a compatibility warning.

ImageResult contains Image blocks, Gemini side Text/Reasoning/Opaque blocks,
finish reason, observed usage, warnings, response metadata and provider metadata.
OpenAI revised_prompt and row extensions stay with the image. Gemini signatures,
Part metadata and media remainders use the same SDK-owned metadata shape as the
language renderer; generated images can be reused as Gemini reference images.
Safety feedback maps to ContentBlocked, preserving its reported raw reason.

OpenAI streams progressive partial images and complete images. Gemini image
streaming is explicitly a buffered fallback: it emits Start with a compatibility
warning, then one complete result. Streams can be cancelled by dropping/cancel().
`aggregate()` returns the final result or the observed error. A clean EOF without
native completion returns Interrupted and retains observed completed images.

## Speech

`Client::synthesize(&SpeechRequest)` takes text, optional voice/instructions/speed/
language, AudioFormat and per-vendor options. OpenAI requires a portable voice or
its typed custom_voice option. Gemini options expose native speechConfig,
speech_metadata and response_format, including multi-speaker configuration.

- OpenAI uses CreateSpeech and native SSE when streaming. Language is unsupported
  and warned. Encoded MP3/Opus/AAC/FLAC/WAV and raw PCM map to native formats. Raw
  PCM is 24 kHz mono; incompatible requested rate/channels are warned. Unsupported
  output encodings use the explicit MP3 default with a warning.
- Gemini uses GenerateContent with AUDIO, speechConfig and response_format.
  Instructions map to Part.speech_metadata.style. Native style/voice/language/
  format options take precedence and warn when overriding portable controls.
  Speed is omitted with a warning. Single/multi-speaker controls and native voice
  union alternatives cannot be set together. Unsupported encodings use WAV with
  a warning; PCM requests pass their sample rate and warn for non-mono channels.

SpeechResult carries an optional Audio block (absent for blocked/interrupted
output without audio), finish reason, usage, warnings, response and provider
metadata. Actual output MIME/rate is retained; no requested model or receipt time
is substituted for missing response metadata. Character usage is returned only
when reported; input text length is not a billing estimate.

`stream_speech` emits Start, AudioChunk carrying an Audio block, and Finish with
usage/metadata/finish reason. Ordinary streaming retains no complete audio buffer.
`aggregate()` deliberately collects chunks into SpeechResult. Multiple URL/mixed
fragments cannot be combined without fetching/transcoding and return InvalidData.
Single-part Gemini native metadata remains compatible with the language renderer;
multiple audio fragments retain their separate metadata under speech_parts.

## Transcription

`Client::transcribe_audio(&TranscriptionRequest)` accepts an Audio block, language,
prompt, Segment/Word timestamp granularity, diarization and native options.
`stream_transcription` emits Start, Delta, Segment, Finish and Error. The complete
result has text, segments, words, optional start/end seconds/speaker labels,
reported language/duration, usage, warnings and response/provider metadata.

- OpenAI requires an encoded inline upload container. A typed
  CreateTranscriptionMultipartForm is encoded with the protocol multipart codec.
  No vendor Files upload is used. JSON is selected normally, verbose_json for
  timestamps, and diarized_json for diarization/known speakers. Diarized output
  warns when word timestamps or prompts cannot be carried. Logprobs are retained
  with JSON; incompatible formats warn and omit them. Verbose streaming preserves
  timestamps through an explicit completed-response fallback with a warning.
- Gemini uses GenerateContent and audioTranscriptionConfig. It parses only native
  Part.audio_transcription rather than guessing a transcript from ordinary model
  text. Duration strings such as 0.1s become seconds; malformed/non-finite offsets
  return InvalidData. Segment boundaries use the first/last reported word offsets;
  absent offsets stay unknown. SMART/custom vocabulary cannot be combined with
  timestamps/diarization, and language_auto/language_hints are exclusive. Deprecated
  native controls produce diagnostics. Native controls override portable values.
- Duration and duration-billed audio seconds are separate observations. OpenAI
  usage.seconds is exposed as audio_seconds; no duration is estimated from words,
  file size or requested settings. Unreported Gemini language/duration remain None.

## Assets, errors and persistence

ImageRequest and TranscriptionRequest expose `resolve_assets(DeliveryMode, resolver)`
and, with s3, `resolve_assets_s3`. They return non-serializable prepared snapshots
for the corresponding *_prepared client methods. The original request retains
application references, and URLs are resolved afresh on every preparation.
S3Client exposes generate_images/stream_images/transcribe_audio/stream_transcription
with the existing reference-to-AssetData loader, and synthesize/stream_speech for
text-only requests. OpenAI transcription stays inline even with PreferUrl/S3.

AssetDialect now distinguishes OpenAI/Gemini image and transcription operations.
The table reuses documented Gemini per-file limits; the OpenAI checked-in
transcription endpoint does not specify a byte cap, so none is invented. Direct
URLs/bytes are checked and used as-is. Input-kind mismatches, unrepresentable masks,
raw PCM uploads to OpenAI transcription and unresolved references return errors.

Structured HTTP/in-band errors retain observed status and original body. Malformed
records and transport faults finish Interrupted; in-band errors finish Error. No
automatic retries or failover are performed. Gemini split-image usage is summed
only where every call reports the counter; individual call observations remain
in provider metadata. Output enums preserve unknown strings or raw JSON values.
The persisted Conversation V1 shape and native passthrough signatures are unchanged.

These are offline mapping/codec/transport tests. They do not establish live model
support, storage reachability, browser CORS or Workers runtime behavior.
