---
title: OrcaTerm
description: "Use an OrcaTerm account through its desktop LightAI protocol: browser OAuth, token refresh, account quota, live model discovery and text conversations."
---

The `orcaterm` channel uses the LightAI protocol shipped in OrcaTerm 1.6.6.
It supports text conversations, including system instructions, history,
streamed answers and reasoning. GPROXY exposes OpenAI Chat Completions and
uses its normal conversions for Responses, Claude Messages and Gemini.

## Set up

1. Create a provider with channel **OrcaTerm**. Leave the base URL empty for
   the China service. Enable `international` for an international account.
2. Choose **Device authorization** (the default login method) and open the
   authorization link. Complete the provider's browser login. GPROXY polls
   automatically and saves the resulting OAuth credential; no device code,
   Cookie or callback URL needs copying. China defaults to WeChat and the
   international service defaults to Google. `oauth_provider` can select
   `wechat`, `txcloud` (Tencent Cloud), `google` or `github`; availability is
   controlled by OrcaTerm for the selected region.
3. Refresh the provider's model catalogue. Model discovery reads the desktop's
   live public Rainbow configuration. Keep model IDs exactly as returned,
   including prefixes such as `TokenHub/` and `Hunyuan3/`.
4. Assign access to the imported models and call GPROXY normally.

The credential is an OrcaTerm account session, not a Tencent Cloud SecretId or
an API key from an individual model vendor. Existing sessions use the normal credential import with `authKind: "oauth"` and
`secret: {"cookie": "ot_session=...; skey=..."}` (retain `skey` when present),
or `secret: {"access_token": "..."}`. The Console initializes the saved record
through the standard refresh action and records its public account identity.
Refresh uses `OAuthRefreshToken` with the current access token, matching the
client, rather than inventing a separate refresh-token grant.

`base_url` overrides the LightAI origin (without `/assistant`). `site_url`
overrides the account origin for login and refresh. The defaults are:

| Region | LightAI | Account |
| --- | --- | --- |
| China | `https://lightai.cloud.tencent.com` | `https://orcaterm.com` |
| International | `https://lightai.tencentcloud.com` | `https://orcaterm.ai` |

## Account quota

The credential's quota query uses `POST /cgi/api?action=DescribeUserUsageStats`
with `{"feature":"hunyuan_ai"}` in the RPC `data`. It reports the desktop's
**AI question allowance**, with used, limit, remaining and used percentage.
The `-1` limit means unlimited; absent counters remain unknown. Empty,
malformed or unrelated feature replies are errors, not a zero balance.

The public `DescribeProductEditions` response checked on 2026-10-24 advertised
1000 questions/month for basic, 6000/month for professional, and daily windows
for trial/domain plans. These are public plan definitions, not a reading of a
particular account, and are not hard-coded into quota queries. No money or
token balance is inferred from this request-count meter.

Only explicit, timezone-qualified `quotaStartDate` / `quotaEndDate` timestamps
on the AI stats are exposed as period bounds. The channel does not infer a
reset from a date without a timezone, a subscription expiry or a fixed monthly
schedule. There is no automatic purchase or quota-reset operation.

The meter does not identify which current LightAI model routes share it, so
this is an **observe-only** account reading. It appears in the normal quota
view but does not locally block every model or count gateway calls against an
assumed pool. Upstream exhaustion responses still apply normally.

The anonymous usage request returned `VERIFY_USER_FAILED`; actual remaining
allowance requires a logged-in account and has not yet been live-verified.

## Supported boundary

This channel currently supports **text only**. It rejects image/audio/file
content, client function tools, tool history, forced tool choice, structured
output modes and multiple choices. An upstream request for an OrcaTerm tool
is an error; GPROXY does not execute a terminal command or acknowledge a tool
approval on behalf of the caller.

Each request uses a fresh upstream conversation ID and sends the caller's
history explicitly. Independent downstream conversations do not share an
upstream session. This is not a zero-retention mode: OrcaTerm controls its
server-side history retention. The channel does not import connections,
credentials for SSH, terminal recordings or desktop telemetry.

The audited SSE wire reports no verified token-count fields. Usage remains
unreported rather than being presented as zero. Account quota is queried
separately; no paid-usage, quota-reset or WebSocket capability is advertised. The discovery path covers built-in models;
account-specific third-party model configurations are not exposed as if they
were built-in model IDs. Downstream model lists still come from GPROXY's
local, permission-filtered catalogue.

The model directory and anonymous OAuth entry points were verified on
2026-10-24: China WeChat/Tencent Cloud and international Google returned their
provider authorization redirects. An uncompleted `OAuthExchangeToken`
returned `TOKEN_EXPIRED`, so the channel treats that result as pending within
its local ten-minute browser-login window. Other errors are not hidden as
pending. A successful grant, generation and refresh still require a real
account; the authenticated end-to-end flow has not been live-verified. Scripted
transport checks establish request/response handling, not upstream account
entitlement or an authenticated end-to-end pass. International endpoint
selection follows the desktop branch; it is not an independently captured
international installation.

## Wire evidence

Official Windows x64 standard installer:

```text
https://orcaterm-desktop-1258344699.cos.ap-guangzhou.myqcloud.com/release/latest/OrcaTerm-latest-win-x64.exe
SHA-256 f42940c099bc32fbe668adfea25fd5353f2b711b1e705536638aa1f78b28c173
```

All 450 entries of its contiguous PE resource table were extracted and Brotli
decoded. Locally recovered sources are under `samples/orcaterm-1.6.6/` (ignored
vendor artifacts). Important webpack modules and formatted-source positions:

| Source | Contract |
| --- | --- |
| Module `2481`, `callSubAgent` and `createCallHandler` | `/assistant/chat`, `input.historyMessages`, UUIDs, `name`, `mode`, `csrfCode`, and SSE request headers |
| Module `26196` | `data:` stream framing, content/reasoning deltas, `[DONE]`, `[ERROR]:...`, `[ABORTED]` |
| Modules `82025`, `92054` | Input action enum and `user.setting` envelope |
| `features.js:40277` | Agent `completion.taskCompletion`, `ask.question`, `review.tool` and thinking projection |
| `features.js:6660` | Regional LightAI origins |
| `features.js:14836` | Model thinking parameters in `modelDesc.kwargs` |
| `features.js:818` | Public Rainbow request and `orcaterm.内置模型` group |
| `features.js:675` | Positive-weight model rows and per-account weighted variant selection |
| `app.js:6210`, `features.js:87360` | Browser authorize URL, session UUID and desktop callback handoff |
| `oauth-browser-redirects.json`, `oauth-pending-session.json` | Anonymous live redirects and pre-authorization exchange response |
| `ot_setting.js`, `Kt` / `Zt`; `features.js:94967`, `77060` | AI stats request, used/limit, unlimited sentinel and question-count display |
| `DescribeProductEditions-anonymous.json` | Public plan periods (`update_span`, `update_unit`) and advertised caps |
| `DescribeUserUsageStats-anonymous.json` | Usage endpoint requires account authentication |
| `features.js:87182` | `GetUserInfo` identity fields |
| `features.js:88116`, `88231`, `88310` | `ot_session`, OAuth exchange/refresh and expiry |
| `features.js:88370` | Definitive session/refresh-token rejection codes versus transient retries |
| `app.js:2920`, `2057` | Account RPC envelope and token-refresh action |
| `http-interceptor.js` | Desktop identity and cookie injection; product version in the literal UA remains `1.0.0` |

Public catalogue request, with no credentials:

```http
POST https://dnspod.cloud.tencent.com/ajax/rainbow?action=getRainbowConfig
Content-Type: application/json

{"app":"ai","groups":["orcaterm.内置模型"],"options":{"enableEnv":1}}
```

The response's `model` field is the upstream selector; `displayName` and `text`
are labels. Disabled (`weight=0`) rows are excluded. No hard-coded snapshot is
substituted after a discovery failure.
