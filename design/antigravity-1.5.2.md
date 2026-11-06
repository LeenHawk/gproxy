# Antigravity CLI 1.5.2 / hub 2.29.0 identity and wire audit (2026-11-06)

All fetches went through `http://100.64.1.4:10808`. The capture ran fully
offline against a local MITM proxy with synthetic credentials only.

## Sources

| What | Source | Latest | Channel before |
|---|---|---|---|
| IDE (Linux x64) | `antigravity-ide-auto-updater-974169037036.us-central1.run.app/releases` | 2.6.0 (`5526304964083712`) | 2.6.0 |
| CLI the IDE's `agy` extension downloads | `antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/linux_amd64.json` | 1.5.2 (`1.5.2-4951595792859136`) | 1.4.10 |
| Hub server inside the CLI | `antigravity --hub` banner | 2.29.0 | 2.28.1 |
| Standalone Antigravity hub app | `antigravity-hub-auto-updater-974169037036.us-central1.run.app/releases` | 2.29.0 (`5723181030440960`) | n/a |

The download page now lists the standalone hub app (`antigravity-public/antigravity-hub/<ver>-<exec>/linux-x64/Antigravity.tar.gz`) next to the IDE. Its version follows the hub server version built into the CLI. The channel does not impersonate that app, so it was not downloaded.

CLI artefact, in `samples/antigravity-cli-1.5.2/` (ignored by `samples/` in `.gitignore`):

- `cli_linux_x64.tar.gz`
  - sha512 `aa876c0813572efaf85ed47983cc937414705d40490cbdb54f6e255656a4f8744ae38e457823e924fa8270b348124e0521d3cda3fec6824bb2d14b98a7a05baa` (matches the manifest)
  - sha256 `da696a8dc6122a63d5ffbdd21b02c8a62d5361fba0ed7d3bc2d2619889e62711`
- `x/antigravity`: sha256 `da1751e7658469fa80bf36eda2c60ee61216d8e42b735a184382ba31d1f6b88e`. It is a stripped Go binary with no readable build info.
- `capture-2026-11-06.jsonl` holds the captured requests (play.googleapis.com telemetry removed). `clienthello-2026-11-06.jsonl` holds the parsed ClientHellos. `hub-auth-url.txt` holds the hub's sign-in URL.

The IDE did not change, so it was not re-downloaded. These are unchanged: `ideVersion` 2.6.0, google-auth-library 10.3.0, Electron 39.2.3 / Node v22.21.1, the editor user agent, and the Electron-side OAuth login (client, secret, five scopes, `localhost:51121/oauth-callback`, no PKCE).

## Capture

Setup:

- Proxy: a Python CONNECT proxy that minted per-host certificates from a throwaway CA and offered ALPN `http/1.1`.
- Client environment: `SSL_CERT_FILE` pointed at that CA, and `HOME` was isolated.
- Hub launch: `AGY_ENABLE_HUB=1 antigravity --hub --hub-port=N --app_data_dir=antigravity --csrf_token=…`.
- Sign-in: done through the hub web UI. An `xdg-open` shim caught the auth URL, then a fake code was sent to the loopback callback.
- Mocked responses: token, userinfo, `loadCodeAssist` and a two-model catalogue (Gemini 3.8 Flash `MODEL_PLACEHOLDER_M319` and Claude Sonnet 5.5 `MODEL_PLACEHOLDER_M404`). The catalogue needed `agentModelSorts` for the UI to offer the models.
- Turns: one Gemini turn from the hub UI, and one Claude turn through `antigravity -p … --model claude-sonnet-5-5-medium` sharing the same HOME.

### What changed

User agent of the hub:
`antigravity/hub/2.28.1 (aidev_client; os_type=linux; arch=amd64; cl=1009308193)` became
`antigravity/hub/2.29.0 (aidev_client; os_type=linux; arch=amd64; cl=1015175089)`.
The format is unchanged and still has no `auth_method`. The standalone CLI sends `antigravity/cli/1.5.2 (aidev_client; os_type=linux; arch=amd64; cl=1015175089; auth_method=consumer)`.

### What did not change

Hosts and calls:
- The first `loadCodeAssist` goes to production `cloudcode-pa`. After a consumer sign-in, everything goes to `daily-cloudcode-pa`.
- The calls and bodies are unchanged:
  - `fetchAvailableModels {project}`
  - `retrieveUserQuotaSummary {project}`
  - `loadCodeAssist {metadata:{ideType:ANTIGRAVITY}}`
  - `fetchUserInfo`, `fetchAdminControls` and `listExperiments`, which take `{project}`
- New-looking GETs `/v1internal/buildWithGooglePlugins` and `/v1internal/cascadeNuxes` are UI/plugin listings and have no channel counterpart.

Generation headers: `User-Agent`, `Authorization: Bearer`, `Content-Type: application/json`, `Accept-Encoding: gzip`, and a chunked body. There is no `Accept` and no `x-goog-api-client`.

Generation envelope: `{project, requestId, request, model, userAgent:"antigravity", requestType}`.
- `requestId` is `agent/<cascade>/<ms>/<trajectory>/<step>` (step 1 for the first turn).
- `request` holds `contents`, `generationConfig`, `labels`, `sessionId`, `systemInstruction` and `tools`. There is no `toolConfig` and no `safetySettings`.
- `sessionId` is still `-3750763034362895579`.
- The label set is exactly what `agent_labels` builds: `cascade_id`, `last_step_index`, `model_enum`, `request_id` (`<trajectory>-<model turns>`), `root_cascade_id`, `trajectory_id`, `used_claude`, `used_claude_conservative` and `used_non_gemini_model`. For Claude the three `used_*` labels read `true` and `model_enum` is `MODEL_PLACEHOLDER_M404`.
- The title/summary call is still `checkpoint/<uuid>` with `requestType: checkpoint` and no labels.

Claude: function declarations still use the proto `parameters` field (upper-case `OBJECT`/`INTEGER` types) and never `parametersJsonSchema`. The hub's own `generationConfig` is `maxOutputTokens: 65535`, `thinkingConfig {includeThoughts: true, thinkingBudget: 1024}`. The schema, history and limit rules in `antigravity/claude.rs` are server-side and were not affected.

TLS ClientHello to `daily-cloudcode-pa` is identical to the channel profile:
- Same cipher order.
- Groups X25519MLKEM768, SecP256r1MLKEM768, SecP384r1MLKEM1024, x25519, P-256, P-384, P-521.
- Same sigalgs, including ML-DSA.
- Extension order `0,11,65281,23,18,5,10,13,50,43,51`. The `50` (signature_algorithms_cert) and the two non-X25519 hybrids remain the known BoringSSL gaps.
- No ALPN on Code Assist.

### Hub sign-in, for reference only

The hub's sign-in uses the same client id and secret but differs from the IDE login the channel impersonates. It adds PKCE S256, redirects to `http://localhost:<random>/auth/callback`, and uses `accounts.google.com/o/oauth2/auth` with a sixth scope, `https://www.googleapis.com/auth/aicode`. The token exchange user agent is `Go-http-client/1.1`.

Login stays on the IDE's Electron flow (IDE 2.6.0 is unchanged), so the channel's OAuth is left as is. No 1.4.10 binary was at hand, so it is unverified whether the `aicode` scope is new in 1.5.2.

## Changelog 1.4.11–1.5.1, wire-relevant only

- Retries on 429/5xx now honour `Retry-After`.
- Outdated thinking blocks are dropped client-side after the system prompt or tools change, on a Google Cloud project.
- Images are capped at 2000 px and large PNGs are sent as JPEG.
- `agy acp` offers every model.

None of these changes the request shape that a relay must reproduce.

## Changes

- `crates/gproxy-channel/src/channels/antigravity/mod.rs`: `USER_AGENT` now carries hub 2.29.0, cl 1015175089. The module and constant docs now cite CLI 1.5.2 / hub 2.29.0, and the TLS profile is documented as unchanged in 1.5.2.
- `crates/gproxy-sdk/src/manage/catalog.rs`: the `antigravity` TLS preset's user-agent header and comment are updated to match.

## Not verified

- `MODEL_ENUMS` was not re-checked. The catalogue is account- and server-side, and no real credential was used. The mock reused the existing enum numbers.
- The quota summary bucket shape is server-side and was not re-checked; only the request was captured.
