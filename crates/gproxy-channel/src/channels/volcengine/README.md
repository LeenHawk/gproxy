# Volcengine Ark plans

The `volcengine` feature registers `volcengine_agent` (Agent Plan) and
`volcengine_coding` (Coding Plan). Both use one implementation and independent
product paths. The engine continues to own routing, protocol conversion,
credential selection, catalogue persistence and accounting.

## Credentials and configuration

Use `auth_kind = "api_key"`. For inference alone, the secret is:

```json
{"api_key": "your-plan-api-key"}
```

Agent Plan needs its dedicated API key. Coding Plan personal uses the platform
API key with Coding Plan enabled; Coding enterprise uses its dedicated seat
API key. Their default paths never fall back to the
pay-as-you-go `/api/v3` endpoint.

Automatic model discovery, quota and plan information additionally require
an IAM key pair in the **same credential secret**:

```json
{
  "api_key": "your-plan-api-key",
  "access_key_id": "your-iam-access-key-id",
  "secret_access_key": "your-iam-secret-access-key"
}
```

An optional `session_token` supports temporary IAM credentials. No refresh is
performed. AK/SK are never sent to inference or media-download endpoints; the
API key is never sent to the control plane. Without AK/SK, inference still
works with manually configured model rows; automatic discovery reports the
missing control credential instead of returning an invented catalogue.

Provider config defaults to `{ "edition": "personal" }`. Set
`edition = "enterprise"` for an enterprise account and set credential
metadata `seat_id` for seat quota. Coding enterprise metadata may also carry
`project_name` when the seat is outside the default project.

The optional origin overrides are:

| Setting | Default | Purpose |
| --- | --- | --- |
| Provider `base_url` | `https://ark.cn-beijing.volces.com` | Inference origin, without a product path |
| `config.control_base_url` | `https://ark.cn-beijing.volcengineapi.com` | IAM-signed Ark control API, region `cn-beijing` |
| `config.speech_base_url` | `https://openspeech.bytedance.com` | Agent Plan HTTP speech |

`config.headers` supplies static inference headers. Operation endpoint overrides
remain complete method URLs. Model discovery overrides must include the control
API's `Action` and `Version` query parameters.

## Operations

| Operation | Agent Plan upstream path | Coding Plan upstream path |
| --- | --- | --- |
| Chat Completions | `/api/plan/v3/chat/completions` | `/api/coding/v3/chat/completions` |
| Responses | `/api/plan/v3/responses` | `/api/coding/v3/responses` |
| Claude Messages | `/api/plan/v1/messages` | `/api/coding/v1/messages` |
| Embeddings | `/api/plan/v3/embeddings` | `/api/coding/v3/embeddings` |
| Images | `/api/plan/v3/images/generations` | Unsupported |
| Video tasks | `/api/plan/v3/contents/generations/tasks[/{id}]` | Unsupported |
| Speech | Speech origin `/api/v3/plan/tts/unidirectional` | Unsupported |

Text generation supports both ordinary and streamed replies, preserving the
native tools, thinking and multimodal fields. Native Responses Web Search is
passed through. Other text dialects use the engine's conversion rules.

Model discovery uses `ListArkAgentPlanModel` with the configured edition, or
`ListArkCodingPlanModel`. Model IDs are preserved exactly; single-model queries
search the same live directory. The documented `ark-code-latest` alias can be
configured manually and keeps its upstream selection behavior. The downstream
model list remains the normal permission-filtered local catalogue.

Images preserve Ark JSON extensions, including reference `image` inputs and
model-supported streaming. There is no separate multipart image-edit adapter.

Video creation accepts the gateway's `prompt`, `duration`, `aspect_ratio`,
`resolution`, `frame_images`, `input_references`, and Ark's native `content`
and other JSON fields. `seconds` also maps to integer `duration`. Use
`resolution` and `aspect_ratio` instead of Sora's pixel `size`, and references
instead of Sora multipart `input_reference`. Query/list responses map statuses
and expose `unsigned_urls` alongside the original `content`. Native `page_num`,
`page_size` and `filter.*` query parameters pass through. DELETE returns
`{ "id": "...", "accepted": true, "action": "cancel_or_delete" }` because
Ark does not say whether it cancelled a queued task or deleted a finished one.
Downloads first query the task and then fetch its URL without any API credential.

HTTP speech accepts model `doubao-seed-tts-2.0`, `input`, an Ark speaker ID in
`voice`, `speed` between 0.5 and 2.0, and `response_format` of `mp3`, `opus` or
`pcm`. Native options such as sample rate go in `req_params.audio_params`;
other `req_params` options are retained. `instructions` maps to the documented
TTS `additions.context_texts`. The audio is returned after successful synthesis;
`stream_format: "sse"`, WAV, realtime TTS and ASR are not implemented. The
upstream's `usage.text_words` becomes `audio_characters`, not token estimates.

## Quota and plan information

Agent personal quota uses `GetAFPUsage`; enterprise quota uses `GetSeatAFPUsage`
for the credential's seat. Values retain AFP precision and upstream millisecond
window boundaries. Text/embedding 5-hour and weekly windows are separate from
the daily visual/speech window; the monthly window covers all supported
inference operations.

Agent credentials may set metadata `allow_paid_usage: true` to permit routing
past subscription windows when the account has independently enabled upstream
postpaid usage. This does not enable billing upstream. Visual daily caps remain
blocking; speech with postpaid enabled is governed by the monthly window.

Coding personal uses `GetCodingPlanUsage`, the same OpenAPI used by official
ArkCLI `usage plan --product coding-plan`. It reports used percentages for the
5-hour (`session`), weekly and monthly windows. Amounts use base-plan percentage
points: used is `Percent`, total is `Cap` (default 100), and remaining is
`max(Cap - Percent, 0)`. Progress is normalized against `Cap`, so bonus quota
above 100 does not trigger premature exhaustion. No absolute token/request
balance is reported. Reset timestamps are converted from seconds to milliseconds;
nonpositive sentinels such as `-1`
mean no active reset time. An empty `QuotaUsage` means no active subscription.
Exhausted windows participate in the existing credential quota routing.

Coding enterprise uses `GetSeatInfoUsage` and reports its observed usage without
inventing a unit, total quota or remaining amount. Both editions use the same
IAM AK/SK signing as model discovery.

`GET /{provider}/api/volcengine/plan` is a read-only service calling
`GetPersonalPlan` for the channel's product. Use the provider's configured name
and the administrator header `x-gproxy-view: credential:{id}`. It exposes the
personal plan tier, status, dates and auto-renew flag under that **credential
view** only. It is not a remaining-quota endpoint; caller and pool views receive 403.

The official sources are linked in each module. The signing regression checks
vectors generated independently by the official Python SDK; it does not
establish live account entitlement or real service availability.
