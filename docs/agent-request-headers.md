# Agent CLI 请求头参考

这页记录 v2 中 agent 类 channel 的请求头伪装目标。它不是运行时配置说明，而是维护
`src/channel/bulletins/*` 时用的对照表：哪些头由 gproxy 注入，哪些头来自凭证，哪些头
由传输层自动生成，哪些动态头需要保持关联。

采集脚本：

- `scripts/capture_headers.py`：转发型 MITM，记录有序 HTTP headers。
- `scripts/capture_fwd_mitm.py`：需要真实上游响应才能继续执行的 channel 使用。

采集样本中的 `Authorization`、Bearer token、cookie、AWS/OIDC token 都必须脱敏后再写入
文档。

## 分类规则

| 类别 | 含义 | 文档处理 |
| --- | --- | --- |
| 静态 | 固定值或随 channel 版本固定 | 写入 channel 代码或默认 fingerprint。 |
| 半静态 | 随 CLI/SDK/Node/OS 版本漂移 | 写明采集版本，升级时复核。 |
| 动态 | 每请求、每会话或每机器变化 | 记录生成关系，不能写死。 |
| 凭证 | OAuth/API token/cookie | 只说明来源，不记录真实值。 |
| 传输 | `Host`、`Content-Length`、`Accept-Encoding`、`Transfer-Encoding` 等 | 通常交给 HTTP client。 |

## claudecode

目标：`POST https://api.anthropic.com/v1/messages?beta=true`

实现位置：

- `src/channel/bulletins/claudecode/auth.rs`
- `src/channel/bulletins/claudecode/cch.rs`
- `src/channel/bulletins/claudecode/mod.rs`

| 头 | 目标形态 | v2 行为 |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | 从 Claude Code OAuth/cookie secret 注入。 |
| `anthropic-version` | `2023-06-01` | 静态注入。 |
| `anthropic-beta` | OAuth marker + client beta，去重 | `oauth-2025-04-20` 放在最前；转发客户端 beta。 |
| `anthropic-dangerous-direct-browser-access` | `true` | 静态注入。 |
| `x-app` | `cli` | 静态注入。 |
| `user-agent` | `claude-cli/2.1.162 (external, cli)` | 静态注入。 |
| `x-claude-code-session-id` | UUIDv4 形状，会话相关 | 由 `cch::session_id` 派生，和 body `metadata.user_id.session_id` 相同。 |
| `x-client-request-id` | UUID，每请求 | 仅默认 `api.anthropic.com` 的模型调用注入。 |
| `x-stainless-*` | JS SDK 指纹头 | 静态注入；升级 Claude Code SDK 时需要复核版本。 |

注意：

- CCH 只对精确 `POST /v1/messages` 生效，不作用于
  `POST /v1/messages/count_tokens`。
- `x-stainless-package-version` 和 `x-stainless-runtime-version` 是最容易随
  Claude Code 版本漂移的字段。当前代码仍按已实现常量注入，采集到新版后应和测试一起更新。

## codex

目标：`POST https://chatgpt.com/backend-api/codex/responses`

实现位置：

- `src/channel/bulletins/codex/auth.rs`
- `src/channel/bulletins/codex/mod.rs`
- `src/channel/bulletins/codex/fingerprint.rs`

| 头 | 目标形态 | v2 行为 |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | 从 Codex OAuth secret 注入。 |
| `accept` | `text/event-stream` | 静态注入；body 会强制 `stream:true`。 |
| `content-type` | `application/json` | 静态注入。 |
| `originator` | `codex_exec` | 静态注入，需和 UA 同步。 |
| `user-agent` | `codex_exec/0.137.0 ...` | 静态注入。 |
| `session-id` | UUIDv7，会话级 | 如果客户端已提供则保留；否则 v2 生成 fallback。 |
| `x-client-request-id` | 通常等于 `session-id` | 如果客户端已提供则保留；否则与 fallback `session-id` 同值。 |
| `thread-id` | UUIDv7 | 允许从客户端转发；v2 不强制生成。 |
| `x-codex-window-id` | `<thread-id>:0` | 允许从客户端转发。 |
| `x-codex-beta-features` | 功能列表 | 允许从客户端转发。 |
| `x-codex-turn-metadata` | JSON，含 turn id、时间、workspace、sandbox 等 | 允许从客户端转发。 |
| `chatgpt-account-id` | account id | 从 `id_token` claim 解出后注入。 |

v2 的策略是：gproxy 负责认证、UA、originator 和 body normalization；Codex-aware
客户端自己提供的 session/turn metadata 头优先保留，因为这些头内部有关联关系。
当普通 OpenAI Responses 客户端没有这些头时，v2 只补齐 backend 必需的
`session-id` 与 `x-client-request-id` 配对。

## geminicli

目标：`POST https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse`

实现位置：

- `src/channel/bulletins/geminicli/auth.rs`
- `src/channel/bulletins/geminicli/mod.rs`

| 头 | 目标形态 | v2 行为 |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | 从 Gemini CLI OAuth secret 注入。 |
| `content-type` | `application/json` | 静态注入。 |
| `accept` | `*/*` | 静态注入。 |
| `user-agent` | `GeminiCLI-tui/0.46.0/<model> (linux; x64; terminal) google-api-nodejs-client/9.15.1` | 按上游模型生成。 |
| `x-goog-api-client` | `gl-node/22.20.0` | 静态注入。 |

Gemini CLI 模型路径没有已知 session/request id 类动态头。模型名是 UA 的一部分，修改
模型映射时要保留这个关系。

## antigravity

目标：`POST https://(daily-)cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse`

实现位置：

- `src/channel/bulletins/antigravity/auth.rs`
- `src/channel/bulletins/antigravity/mod.rs`

| 头 | 目标形态 | v2 行为 |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | 从 Antigravity OAuth secret 注入。 |
| `content-type` | `application/json` | 静态注入。 |
| `user-agent` | `antigravity/cli/1.0.6 linux/amd64` | 静态注入。 |

真实 CLI 模型调用是极简头集合。不要额外注入旧笔记里的 `requestId`、
`requestType` 或 `accept`，这些不是当前 CLI 模型路径会发的头。

## copilotcli

目标：GitHub Copilot CLI chat/model API。

实现位置：

- `src/channel/bulletins/copilotcli/auth.rs`
- `src/channel/bulletins/copilotcli/mod.rs`
- `src/channel/bulletins/copilotcli/usage.rs`

| 头 | 目标形态 | v2 行为 |
| --- | --- | --- |
| `authorization` | `Bearer <copilot_token>` | 先用 GitHub token 换 Copilot token，再注入。 |
| `content-type` | `application/json` | 静态注入。 |
| `user-agent` | `copilot/1.0.61 (linux v24.16.0) term/unknown` | 静态注入。 |
| `copilot-integration-id` | `copilot-developer-cli` | 静态注入。 |
| `editor-version` | `copilot/1.0.61` | 静态注入。 |
| `openai-intent` | `conversation-agent` | 静态注入。 |
| `x-github-api-version` | `2026-06-01` | 静态注入。 |
| `x-client-machine-id` | UUIDv4，每机器稳定 | 从 credential 派生，随凭证稳定。 |
| `x-interaction-id` | UUIDv4，每交互 | 每次请求生成。 |
| `x-initiator` | `user` 或 `agent` | 根据 body 中是否已有 assistant/tool turn 判断。 |

Copilot 的 usage/token-exchange 路径不是模型路径，头集合不同，不要把 usage 头复制到
chat 请求里。

## kiro

目标：Kiro CLI 的 `*.kiro.dev` Smithy/AWS-JSON 1.0 服务。

实现位置：

- `src/channel/bulletins/kiro/mod.rs`
- `src/channel/bulletins/kiro/model_list.rs`
- `src/channel/bulletins/kiro/usage.rs`
- `src/channel/bulletins/kiro/auth/*`

| 头 | 目标形态 | v2 行为 |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | social、Builder ID 或 IDC 登录 secret 刷新后注入。 |
| `content-type` | `application/x-amz-json-1.0` | 静态注入。 |
| `accept` | `*/*` | 静态注入。 |
| `user-agent` | `aws-sdk-rust/... api/codewhispererstreaming/... app/AmazonQ-For-CLI` | runtime/model 路径注入 streaming UA。 |
| `x-amz-user-agent` | 同 UA | 静态注入。 |
| `x-amz-target` | Smithy operation | 按 runtime、model list、usage 操作选择。 |
| `x-amzn-codewhisperer-optout` | `false` | 静态注入。 |
| `amz-sdk-request` | `attempt=1; max=3` | 静态注入。 |
| `amz-sdk-invocation-id` | UUIDv4，每请求 | 每次请求生成。 |

Kiro 当前不是 SigV4。不要注入 `x-amz-date`、`x-amz-security-token` 或
`x-amzn-kiro-agent-mode`。

## 更新流程

升级某个真实 CLI 后，按这个顺序更新：

1. 用相同 prompt 采集模型路径 headers，并保存脱敏样本。
2. 区分模型路径、登录路径、usage 路径和遥测路径，不要混用。
3. 标出静态、半静态、动态、凭证、传输头。
4. 对动态头确认关联关系，例如 Codex 的 session/window/turn metadata，Claude 的
   session id 与 CCH body metadata。
5. 更新 `src/channel/bulletins/<channel>/` 的注入逻辑。
6. 更新本页和 `docs/agent-tls-fingerprints.md` 中的 UA/TLS 对应关系。
7. 跑对应 channel 的单测，至少覆盖 header 注入和不应注入的反例。

请求头伪装和 TLS/HTTP2 指纹要一起维护。只更新 UA 不更新 fingerprint，或只更新
fingerprint 不更新 UA，都会让模型路径呈现不一致的客户端画像。

## English

# Agent CLI Request Header Reference

This page records the request-header impersonation targets for agent-style
channels in v2. It is not runtime configuration documentation. It is the
maintenance reference for `src/channel/bulletins/*`: which headers gproxy
injects, which come from credentials, which are generated by the transport
layer, and which dynamic headers must keep internal relationships.

Capture scripts:

- `scripts/capture_headers.py`: forwarding MITM that records ordered HTTP
  headers.
- `scripts/capture_fwd_mitm.py`: for channels that need a real upstream response
  before they continue.

Any captured `Authorization`, bearer token, cookie, AWS token, or OIDC token
must be redacted before it is written into documentation.

## Classification

| Class | Meaning | Documentation rule |
| --- | --- | --- |
| Static | Fixed value, or fixed for one channel version | Put it into channel code or the default fingerprint. |
| Semi-static | Drifts with CLI/SDK/Node/OS version | Record the capture version and re-check on upgrades. |
| Dynamic | Changes per request, session, or machine | Record generation relationships; never hard-code. |
| Credential | OAuth/API token/cookie | Describe the source only; never record real values. |
| Transport | `Host`, `Content-Length`, `Accept-Encoding`, `Transfer-Encoding`, etc. | Usually leave it to the HTTP client. |

## claudecode

Target: `POST https://api.anthropic.com/v1/messages?beta=true`

Implementation:

- `src/channel/bulletins/claudecode/auth.rs`
- `src/channel/bulletins/claudecode/cch.rs`
- `src/channel/bulletins/claudecode/mod.rs`

| Header | Target shape | v2 behavior |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | Injected from Claude Code OAuth/cookie secret. |
| `anthropic-version` | `2023-06-01` | Static injection. |
| `anthropic-beta` | OAuth marker plus deduped client beta | `oauth-2025-04-20` comes first; client beta is forwarded. |
| `anthropic-dangerous-direct-browser-access` | `true` | Static injection. |
| `x-app` | `cli` | Static injection. |
| `user-agent` | `claude-cli/2.1.162 (external, cli)` | Static injection. |
| `x-claude-code-session-id` | v4-shaped UUID, session-related | Derived by `cch::session_id`; equals body `metadata.user_id.session_id`. |
| `x-client-request-id` | UUID, per request | Injected only for model calls to default `api.anthropic.com`. |
| `x-stainless-*` | JS SDK fingerprint headers | Static injection; re-check versions when upgrading Claude Code SDK. |

Notes:

- CCH applies only to exact `POST /v1/messages`, not
  `POST /v1/messages/count_tokens`.
- `x-stainless-package-version` and `x-stainless-runtime-version` are the fields
  most likely to drift with Claude Code versions. Update constants and tests
  together after a fresh capture.

## codex

Target: `POST https://chatgpt.com/backend-api/codex/responses`

Implementation:

- `src/channel/bulletins/codex/auth.rs`
- `src/channel/bulletins/codex/mod.rs`
- `src/channel/bulletins/codex/fingerprint.rs`

| Header | Target shape | v2 behavior |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | Injected from Codex OAuth secret. |
| `accept` | `text/event-stream` | Static injection; body is forced to `stream:true`. |
| `content-type` | `application/json` | Static injection. |
| `originator` | `codex_exec` | Static injection; keep in sync with UA. |
| `user-agent` | `codex_exec/0.137.0 ...` | Static injection. |
| `session-id` | UUIDv7, session-level | Preserved when supplied by client; otherwise generated as fallback. |
| `x-client-request-id` | Usually equals `session-id` | Preserved when supplied by client; otherwise equals fallback `session-id`. |
| `thread-id` | UUIDv7 | Allowed through from client; v2 does not force-generate it. |
| `x-codex-window-id` | `<thread-id>:0` | Allowed through from client. |
| `x-codex-beta-features` | Feature list | Allowed through from client. |
| `x-codex-turn-metadata` | JSON with turn id, time, workspace, sandbox, etc. | Allowed through from client. |
| `chatgpt-account-id` | Account id | Decoded from `id_token` claim and injected. |

v2 owns authentication, UA, originator, and body normalization. Session and turn
metadata supplied by a Codex-aware client are kept because those headers have
internal relationships. For ordinary OpenAI Responses clients, v2 only fills the
backend-required `session-id` / `x-client-request-id` pair.

## geminicli

Target: `POST https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse`

Implementation:

- `src/channel/bulletins/geminicli/auth.rs`
- `src/channel/bulletins/geminicli/mod.rs`

| Header | Target shape | v2 behavior |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | Injected from Gemini CLI OAuth secret. |
| `content-type` | `application/json` | Static injection. |
| `accept` | `*/*` | Static injection. |
| `user-agent` | `GeminiCLI-tui/0.46.0/<model> (linux; x64; terminal) google-api-nodejs-client/9.15.1` | Generated from upstream model. |
| `x-goog-api-client` | `gl-node/22.20.0` | Static injection. |

The Gemini CLI model path has no known session/request-id dynamic headers. The
model id is part of the UA, so preserve that relationship when changing model
mapping.

## antigravity

Target: `POST https://(daily-)cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse`

Implementation:

- `src/channel/bulletins/antigravity/auth.rs`
- `src/channel/bulletins/antigravity/mod.rs`

| Header | Target shape | v2 behavior |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | Injected from Antigravity OAuth secret. |
| `content-type` | `application/json` | Static injection. |
| `user-agent` | `antigravity/cli/1.0.6 linux/amd64` | Static injection. |

The real CLI model request uses a minimal header set. Do not add old-note
headers such as `requestId`, `requestType`, or `accept`; the current CLI model
path does not send them.

## copilotcli

Target: GitHub Copilot CLI chat/model API.

Implementation:

- `src/channel/bulletins/copilotcli/auth.rs`
- `src/channel/bulletins/copilotcli/mod.rs`
- `src/channel/bulletins/copilotcli/usage.rs`

| Header | Target shape | v2 behavior |
| --- | --- | --- |
| `authorization` | `Bearer <copilot_token>` | Exchange GitHub token for Copilot token, then inject. |
| `content-type` | `application/json` | Static injection. |
| `user-agent` | `copilot/1.0.61 (linux v24.16.0) term/unknown` | Static injection. |
| `copilot-integration-id` | `copilot-developer-cli` | Static injection. |
| `editor-version` | `copilot/1.0.61` | Static injection. |
| `openai-intent` | `conversation-agent` | Static injection. |
| `x-github-api-version` | `2026-06-01` | Static injection. |
| `x-client-machine-id` | UUIDv4, stable per machine | Derived from the credential and stable for it. |
| `x-interaction-id` | UUIDv4, per interaction | Generated per request. |
| `x-initiator` | `user` or `agent` | Derived from whether the body already has assistant/tool turns. |

Copilot usage/token-exchange paths are not model paths and use different header
sets. Do not copy usage headers into chat requests.

## kiro

Target: Kiro CLI `*.kiro.dev` Smithy/AWS-JSON 1.0 services.

Implementation:

- `src/channel/bulletins/kiro/mod.rs`
- `src/channel/bulletins/kiro/model_list.rs`
- `src/channel/bulletins/kiro/usage.rs`
- `src/channel/bulletins/kiro/auth/*`

| Header | Target shape | v2 behavior |
| --- | --- | --- |
| `authorization` | `Bearer <access_token>` | Injected after social, Builder ID, or IDC login secret refresh. |
| `content-type` | `application/x-amz-json-1.0` | Static injection. |
| `accept` | `*/*` | Static injection. |
| `user-agent` | `aws-sdk-rust/... api/codewhispererstreaming/... app/AmazonQ-For-CLI` | Injected on runtime/model path. |
| `x-amz-user-agent` | Same as UA | Static injection. |
| `x-amz-target` | Smithy operation | Selected per runtime, model-list, and usage operation. |
| `x-amzn-codewhisperer-optout` | `false` | Static injection. |
| `amz-sdk-request` | `attempt=1; max=3` | Static injection. |
| `amz-sdk-invocation-id` | UUIDv4, per request | Generated per request. |

Kiro is not SigV4 in the current path. Do not inject `x-amz-date`,
`x-amz-security-token`, or `x-amzn-kiro-agent-mode`.

## Update Flow

When upgrading a real CLI:

1. Capture model-path headers with the same prompt and save a redacted sample.
2. Separate model path, login path, usage path, and telemetry path.
3. Mark static, semi-static, dynamic, credential, and transport headers.
4. Confirm dynamic relationships, such as Codex session/window/turn metadata or
   Claude session id plus CCH body metadata.
5. Update `src/channel/bulletins/<channel>/` injection logic.
6. Update this page and `docs/agent-tls-fingerprints.md` together.
7. Run channel tests covering header injection and negative cases where headers
   must not be injected.

Header impersonation and TLS/HTTP2 fingerprints must be maintained together.
Updating only UA or only fingerprint makes the model path look like an
inconsistent client.
