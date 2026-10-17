# Trae desktop channel: China and international

Audit date: 2026-10-17. The implementation is a single `trae` channel with
`config.international` (default `false`). It adapts the desktop model API, not
an unattended copy of the IDE or a remote agent that executes tools.

## Reproduce the evidence

Official sites: https://www.trae.cn/ and https://www.trae.ai/.
Their download code calls:

- China: `https://api.trae.cn/icube/api/v1/native/version/trae/cn/latest`
- International: `https://icube-normal.trae.ai/icube/api/v1/native/version/trae/latest`

Both downloaded Linux x64 archives and their metadata are retained locally in
`samples/trae/{china,international}/` (ignored by Git). SHA-256 below identifies
our downloaded samples; the metadata did not provide an independently verified
publisher signature. Neither desktop application was installed or launched.

| Edition | appVersion | tronBuildVersion | SHA-256 of archive |
| --- | --- | --- | --- |
| China | 3.4.3 | 2.3.92182 | `db39d15e8c20a1b660b882acb968a2727a6c31d034e45a976416e7eca7156624` |
| International | 3.6.2 | 2.3.91093 | `c3bcc856dbaac1d87da10cddfa6c7ade97b4780c0bc8944f1b2d03e144540df1` |

Archive URLs:

- `https://lf-cdn.trae.com.cn/obj/trae-com-cn/pkg/app/releases/stable/2.3.92182/linux/TRAE_CN-linux-x64.tar.gz`
- `https://lf-cdn.trae.ai/obj/trae-ai-sg/pkg/app/releases/stable/2.3.91093/linux/TRAE-linux-x64.tar.gz`

Extract the archives under separate directories. Important paths relative to
`resources/app/`:

| Evidence | Facts it establishes |
| --- | --- |
| `product.json` | `bootConfig` origins; `authConfig.TRAE.stable`; app ID; build/version identity |
| `out/main.js` | Electron `oauth/marscode` login, device key generation, refresh, token expiry, account fields |
| `node_modules/@byted-icube/ai-modules-chat/dist/index.mjs` | Generated service methods `GetConfigDetail`, `LLMRawChatV2`, and their request field names |
| `modules/ai-agent/libai_agent.so`, `libharness.so` | Retained Rust API routes, serializer fields and SSE names; these are stripped native libraries, not recovered Rust source |
| `out/vs/workbench/workbench.desktop.main.js` | `ICubeUsageService` entitlement calls, quota fields, unlimited sentinel and timestamp units |

International `formatted/main.js` was produced with the system Babel parser
and generator (syntax formatting, not execution). Useful anchors in that file:
`PPe` creates the device key; `NPe` signs refresh proofs;
`exchangeTokenByAuthCode`, `exchangeTokenByRefreshToken`, `buildLoginUrl`, and
`Q8` map the login result. Keep function names as search anchors because line
numbers depend on formatter version.

The public website bundles are retained under each edition's `web/`. China
`authorization_page.39801783d1.js` and international `6597.d2fb2c9ae3.js` show
`TraeGetPCAuthCode`, `authCodeInfo`, and the redirect to `auth_callback_url`.
They preserve query parameters already present on that callback URL.

Third-party implementations were inspected only to cross-check protocol
variants and identify what needed checking in the official client:
`wangqi233/trae2api` and `autumnsentiment/Trae2api-cn`. Their model aliases,
account pooling, fallback, tool-prompt emulation and local credential-reading
code are not part of this implementation. A third-party test fixture is not
an authenticated capture of this gateway.

## Login, device identity and regions

Both editions use desktop client ID `ono9krqynydwx5` and PKCE S256. The
browser opens the selected edition's `/authorization` with `auth_from=trae`,
`login_channel=native_ide`, `login_version=1`, build identity, device/machine
IDs, the challenge, and a loopback callback URL.

The official page obtains an AuthCode and redirects with
`authCodeInfo={"AuthCode":...}` plus account-routing information. The user
copies the complete address bar URL back to GProxy, even if the loopback page
cannot open. The host validates its login-session state; the channel also
checks the callback's state. No local listener is needed, and the browser can
run on a different machine from GProxy. A bare AuthCode is also accepted by the
channel's completion API, but the full URL preserves region information.

The channel exchanges the code at `/trae/api/v3/oauth/ExchangeToken`:

- `ClientID`, `AuthCode`, `CodeVerifier`, `IDEVersion`, `DeviceInfo`.
- `DeviceInfo` carries a per-login device ID, machine ID and SPKI PEM P-256
  public key. Identifiers are generated for the credential; no real machine
  identifiers or user-local credential files are read.
- The P-256 private scalar is in `provider_secrets.device_key`, never public
  credential metadata. Device and machine IDs remain stable across refresh.
- `GetUserInfo` returns `UserID`, `AIRegion`, and `ScreenName`.
- `TokenExpireAt`, `TokenExpireDuration`, and `RefreshExpireAt` are milliseconds,
  matching the desktop's `Date` arithmetic.

Refresh uses the same endpoint, `ClientSecret: ""`, the refresh token and a
`DeviceProof`. Sign this exact UTF-8 string using SHA-256 and P-256 ECDSA;
encode the DER signature using standard base64:

```text
POST
/trae/api/v3/oauth/ExchangeToken
<client ID>
<refresh token>
<Unix seconds>
<random hex nonce>
```

The existing access token is sent as `x-cloudide-token`. A full credential
replacement preserves the device key and public account facts. The desktop's
invalid-token codes are distinguished from version, device-policy, transport
and server errors: the latter must not mark a credential permanently dead.
Imported access tokens can be used with their account/device metadata, but
cannot be renewed without the matching device key and refresh token.

| Edition / region | Model API | Account API | Quota API |
| --- | --- | --- | --- |
| China | `trae-api-cn.mchost.guru` | `api.trae.cn` | `api.trae.cn` |
| International default | `core-normal.trae.ai` | `grow-normal.trae.ai` | `grow-normal.trae.ai` |
| SG | `coresg-normal.trae.ai` | `growsg-normal.trae.ai` | `growsg-normal.trae.ai` |
| US | `coreva-normal.trae.ai` | `growsg-normal.trae.ai` | `growva-normal.trae.ai` |
| USTTP | `core-normal.traeapi.us` | `grow-normal.traeapi.us` | `grow-normal.traeapi.us` |

Routing is a closed mapping; arbitrary hosts from pasted callbacks are never
used as outbound destinations. `provider.base_url` overrides model/chat origin;
`config.auth_base_url` overrides account/quota origin. A per-operation endpoint
override replaces the final method URL, not the model-configuration preflight.

## Chat and models

The host converts requests to OpenAI Chat before calling this channel.
The native model catalogue is `GetConfigDetail` at
`GET /api/ide/v1/get_detail_param?function=chat_v3&need_prompt=false&poly_prompt=true`.
Its `config_info_list[].config_name` is preserved exactly as the public ID;
no prefix stripping, case folding, invented model aliases or fallback models
are introduced. BYOK configuration groups are excluded from this account-model
catalogue. The normal host still controls the downstream, locally stored model
catalogue and its permissions.

Generation resolves that exact account model and calls
`POST /api/ide/v2/llm_raw_chat`. The selected detail supplies `model_name` and
`encrypted_model_params`; the channel does not decrypt or fabricate that blob.
Messages use array-shaped content, preserving tool call IDs, function schemas
and tool results. Native `tools`, `tool_choice` and `reasoning_effort` fields
are forwarded; tools execute at the downstream client, not inside GProxy.
The backend model configuration governs sampling/output limits; this adapter
does not claim arbitrary Chat sampling parameters are supported by the native
raw endpoint.

Identity includes `Cloud-IDE-JWT`, app ID, desktop version, version code,
device/machine ID, account ID and trace IDs. Downstream authentication and
forged device headers are removed. An incoming allowed `x-conversation-id`
or `x-session-id` is retained; otherwise a new session ID is generated.
No native TLS fingerprint is claimed from static inspection alone.

The upstream stream is translated into standard OpenAI Chat SSE, or collected
for a non-streaming caller. Supported native event forms include
`text_delta`, `reasoning_delta`, `translated_text`, legacy output snapshots,
indexed `tool_calls` / `function_call`, `token_usage`, `done` and `error`.
Queue/progress events do not become assistant text. HTTP non-success responses
retain their status and lazy body. In-band errors stay errors; an unexpected
EOF is not converted into a successful finish.

Token usage snapshots merge reported fields, not sums of cumulative values.
Input/output totals and cache reads/writes become standard Chat usage fields,
so the host's normal usage reader handles both stream and non-stream replies.
Missing usage is left unreported, not estimated or filled with zeros.

## Account quota

`ICubeUsageService` reads `POST /trae/api/v1/pay/ide_user_ent_usage` for the
international edition and `.../v2/...` for mainland credits/fast-request data.
Requests carry `require_usage: true`, `req_source: 1` and account authentication.

`user_entitlement_pack_list` is reported per entitlement, keeping base and
bonus usage, credits, premium fast requests and pay-as-you-go usage separate.
`-1` is unlimited, not zero; entitlement start/end timestamps are seconds.
Missing amounts remain unknown. Basic/bonus/paygo amounts keep native `usage`
units; no unverified exchange rate or dollar conversion is invented.
These are observations with unknown model coverage, not new host admission
limits. No quota reset, purchase, sign-in reward or subscription mutation is
performed.

## Validation and scope

Local scripted checks exercise both editions, region routing, PKCE callback
completion, DER signature verification against the issued public key, refresh
rotation, transient/refused refresh distinction, native model lookup, tool
history, split UTF-8 SSE, repeated deltas, cache usage, quota sentinels,
endpoint overrides, non-2xx preservation and interrupted streams.

The coding-agent guide's native and wasm checks, full channel suite and the
Console's type/lint/i18n checks are run before reporting completion. Disposable
Trae verification fixtures are removed after use, following the existing user
preference. No real Trae account has been used: login authorization, model
entitlements, upstream schema acceptance and charged generation still require
an authenticated smoke test. Local tests do not establish public availability
or CI/release completion.

Not advertised: BYOK provider setup, cloud-agent execution, remote workspaces,
MCP execution, images/video, embeddings, realtime voice, enterprise SSO,
subscription purchases or the IDE's unrelated control-plane services.

Local check results for this implementation:

- `cargo fmt --all` and final formatting check: passed.
- `cargo clippy -p gproxy-channel --all-features --all-targets -- -D warnings`: passed.
- The same library Clippy check for `wasm32-unknown-unknown`: passed.
- `cargo check -p gproxy-channel --no-default-features --features trae --target wasm32-unknown-unknown`: passed. This caught and fixed the independent `getrandom 0.2/js` feature requirement of P-256.
- `cargo test -p gproxy-channel --all-features`: 565 passed at the full-suite run.
- Final disposable Trae checks after the boundary fixes: 5 passed; test file removed.
- `cargo check -p gproxy-app -p gproxy-sdk --features gproxy-sdk/trae`: passed.
- Console TypeScript, scoped ESLint and i18n checks: passed.
