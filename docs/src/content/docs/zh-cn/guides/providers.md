---
title: Provider 与 Channel
description: 在 GPROXY v2 中配置上游 Provider、凭据、Operation 路由能力、代理、TLS 指纹和 scoped 访问。
---

**Provider** 是一个已保存的上游连接，包含名称、Channel 类型、一个或多个凭据、模型目录，
以及可选的路由和请求规则。例如，即使使用相同 Channel，也可以分别创建
`openai-primary`、`openrouter-fallback` 和 `claude-team`。

在控制台保存修改后，新请求会直接使用最新配置，不需要重启 GPROXY。

## 内置 Channel

native 构建包含下表全部渠道；需要多阶段 WebSocket 会话的消费版 web 渠道不进入 edge
构建。当前内置 channel id 包括：

| Channel id | 常见用途 |
| --- | --- |
| `openai`, `custom` | OpenAI API 或 OpenAI-compatible gateway。 |
| `openrouter`, `deepseek`, `groq`, `nvidia`, `vercel` | OpenAI-like 的 API-key provider。 |
| `claudeapi` | Anthropic Claude Messages API。 |
| `aistudio`, `vertex`, `vertexexpress` | Gemini / Vertex 上游；`vertex` 也支持原生 Claude 合作伙伴模型。 |
| `codex`, `claudecode`, `geminicli`, `antigravity`, `grokbuild`, `kiro`, `copilotcli` | OAuth、device-code、cookie 或 envelope 类型的 agent channel。 |
| `chatgpt` | 通过 chatgpt.com 会话 cookie 接入 ChatGPT 消费版 web 后端。 |
| `claudeweb` | 通过 claude.ai 会话 cookie 接入 Claude 消费版 web 后端（仅 native）。 |
| `tasklet` | 通过浏览器会话 token 接入 Tasklet Agent API（仅 native）。 |

每个 channel 都声明 `(Operation, OperationKind) -> RoutingDecision` 的能力表。provider 的默认 `routing_rules` 由这张表生成。因此 v2 的协议能力按 Operation 组织，而不是按 OpenAI / Claude / Gemini provider 家族分桶。

### Vertex Claude 合作伙伴模型

除 Gemini 外，`vertex` channel 也原生接受 Claude 的 `/v1/messages` 和
`/v1/messages/count_tokens` 接口。照常配置服务账号凭据，把 `location` 设为所选 Claude
模型可用的区域，并将 Vertex 模型 ID（例如以 `@YYYYMMDD` 结尾的 ID）填作 route member
的上游模型。GPROXY 会保持 Anthropic 请求与 SSE 响应格式不变，只把调用映射到 Vertex 的
`publishers/anthropic` raw-prediction 端点。模型启用方式、模型 ID 和区域可用性请参考 Google
的[合作伙伴模型概览](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/partner-models/use-partner-models)
与 [Claude 模型文档](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/partner-models/claude)。

在此能力加入前创建的 provider 会保留数据库中已有的路由规则。若要启用原生端点，请重置该
provider 的默认路由，或手动把 Claude Messages 与 Claude count-tokens 规则改为
`passthrough`。

### ChatGPT 渠道（cookie 会话）

`chatgpt` 渠道用浏览器**会话 cookie** 代理 **chatgpt.com 消费版 web 后端** —— 不是
API key、也不是 OAuth。支持普通对话、thinking / pro / 深度研究（流式思维链 + 报告）、
网页搜索、画图/改图。

**凭证怎么获得。** 在浏览器登录 <https://chatgpt.com>，打开开发者工具 → 网络（Network），
点任意一个 `chatgpt.com` 请求，复制它完整的 `Cookie` 请求头。在 console 里新建一个
`chatgpt` provider，用 **Cookie 登录**把这段 cookie 粘进去。gproxy 会用它请求
`/api/auth/session` 换出 access token，并把 Cloudflare / sentinel 反爬状态预热进密封的
secret。之后 gproxy 会在 access token 临近过期时用存着的 cookie 自动续期（JWT 约 10 天，
session cookie 长得多），所以凭据寿命跟着浏览器会话走 —— 只有当 session cookie 本身失效时
才需要重新粘一份。

**会话模式。** 一个 per-provider 设置（`provider_settings.mode`），在 provider 表单里
是一个三选一选择器，控制会话落在哪里：

| 模式 | 行为 |
| --- | --- |
| 普通（Normal） | 持久会话，进你正常的聊天历史。 |
| 临时聊天（Temporary，默认） | 临时聊天 —— 不入历史、不用于训练。 |
| 进项目（Project） | 会话开在一个 ChatGPT**项目**里，按名自动建/找（默认 `gproxy`），方便分组查看。项目名在表单里设。 |

「进项目」与「临时聊天」互斥（项目会话必然是持久的）。当 `mode` 缺省时，旧的
`temporary_chat: true\|false` 布尔仍然兼容生效。

### Tasklet 渠道（会话 token）

仅 native 的 `tasklet` 渠道会为每次生成创建一个 Tasklet Agent，再通过 Tasklet sync
WebSocket 转发 thinking、自主工具执行和最终内容。OpenAI Chat、Responses、Claude
Messages 与 Gemini 生成请求由现有路由转换统一接入。内嵌 base64 图片、音频和文件会在生成
请求内自动上传，不提供单独的下游 Files API；Tasklet 的 `f_...` 文件 ID 会直接传递。远程
附件 URL 不由 gproxy 下载，而是作为任务内容交给 Tasklet 访问。

Tasklet 特征会启用本地 token 计数。当渲染后的 system/developer/messages 文本超过 50,000
个本地 token 时，gproxy 会自动把完整文本上传为 `paste.txt`，消息仅保留
`Read paste.txt.`；已有附件与客户端工具桥接不受影响。

在 Tasklet provider 中点击 **接入**，输入账号邮箱并请求 Tasklet PIN，即可完成登录。
gproxy 会直接向 Tasklet 交换 PIN，通过 `/api/profile` 验证返回的会话，并且只加密保存会话
token 与所选 workspace 元数据。默认优先选择个人 workspace，否则使用 Tasklet 返回的第一
个 workspace。可选 provider 设置包括 `timezone`（默认 `UTC`）和 `emit_tool_trace`（默认
`false`，开启后把工具名称作为 reasoning 输出）。

同一向导还支持通过 Tasklet 官方 OAuth 客户端登录 Google 与 Microsoft 账号。Tasklet 将两者
的 redirect URI 固定为 `https://tasklet.ai/oauth2callback`，并且只会把 popup 结果发给
`tasklet.ai` 同源 opener，因此 gproxy 无法跨域自动接收回调。向导会改在当前标签页打开 OAuth；
授权完成后，从地址栏复制完整回调 URL，返回 gproxy Console 并粘贴到待完成登录中。gproxy 会
先验证回调 host、path 与一次性 state，再交换 code；回调 code 和浏览器状态都不会写入凭据。

仍可手动创建包含 `session_token` 与 `workspace_id` 的凭据。手动获取方式：

1. 登录 Tasklet，打开浏览器开发者工具的 **Network（网络）** 面板，然后向任意 agent
   发送一条消息。如果没有捕获到请求，保持开发者工具开启并刷新页面后重试。
2. 打开 `POST https://api.tasklet.ai/api/sendChatMessage` 请求。
3. 在请求头 `Authorization` 中复制 `Bearer ` 后面的值，填入 `session_token`；不要包含
   `Bearer ` 前缀。
4. 从 JSON 请求体复制 `workspaceId`，填入 `workspace_id`。

也可以在 `/api/sync` WebSocket 的第一条 `connect` 发送消息中找到 `sessionToken`。该 token
等同密码；Tasklet 会话被撤销或过期后，需要在 gproxy 中同步更换。

`channel-tasklet` 特征还会内置一个 Rust MCP 服务，用于把工具调用交还给客户端。先用公网
HTTPS 暴露 gproxy，创建一个专供 Tasklet 使用的 gproxy 用户 API Key，并在 Tasklet 凭据中
加入：

```json
{
  "mcp_url": "https://你的_GPROXY_域名/tasklet/mcp",
  "mcp_api_key": "你的_GPROXY_用户_KEY"
}
```

首次收到携带客户端 tools 的请求时，gproxy 会检查 Tasklet workspace 的连接。如果该 URL
尚未注册，gproxy 会自动创建 Tasklet 要求的 pending MCP setup agent/block，携带
`X-API-Key` 提交配置，验证新连接，并且只给 `gproxy_call_client_tool` 授予 workspace 权限。
已存在的同 URL 连接会直接复用，不再需要手动去 Tasklet 添加连接。检查结果缓存五分钟后重新
验证；MCP 接口不接受查询参数中的 key。

当 OpenAI 兼容请求携带 function 或 custom tools 时，gproxy 会把 schema 与一个短期、
一次性的 turn id 交给 Tasklet。Tasklet 发起 MCP 调用后，gproxy 会向原请求返回标准
`tool_calls`，不会自行执行客户端工具。

MCP 接口要求有效且已启用的 gproxy 用户 Key，并且不会公开凭据或当前工具清单；调用还必须
持有活动 Tasklet turn 中的随机 turn id。撤销这个专用用户 Key 即可关闭 Tasklet MCP 访问。
多实例部署必须确保生成请求与 MCP 回调落到同一个 gproxy 进程，因为活动响应流是进程内状态。

## Provider 字段

| 字段 | 含义 |
| --- | --- |
| `name` | 唯一 provider 名称；scoped 路由会在 URL 中使用它。 |
| `channel` | Channel registry id，例如 `openai` 或 `claudeapi`。 |
| `settings_json` | 自由 JSON 设置，常见字段包括 `base_url`、`endpoints` 和 channel 开关。 |
| `credential_strategy` | 凭据池策略，目前是 `round_robin` 或 `sticky`。 |
| `proxy_url` | native 出站代理；edge 会忽略 native 代理设置。 |
| `tls_fingerprint` | provider 级 TLS/HTTP2 模拟配置；credential 可以覆盖。 |
| `enabled` | 禁用后不会参与路由。 |

常用的 `settings_json` 配置在控制台中有对应表单字段：

| 设置 | 用途 |
| --- | --- |
| `base_url` | Channel 级回退前缀；未配置精确 endpoint 时会追加标准接口路径。 |
| `endpoints` | 可选的最终 URL 覆盖，例如 `{"openai_chat_completions":"https://api.openai.com/v1/chat/completions"}`；优先于 `base_url`，且不会追加路径。动态模型路径可以使用 `{model}`。 |
| `enable_magic_cache` | 识别 GPROXY 缓存触发字符串，并写入 Claude 或 OpenAI 原生缓存断点。适用于 OpenAI、Codex、Claude API、Claude Code、OpenRouter 和 Vercel。 |
| `enable_claude_fable_fallback` | 在支持 Claude 的 Channel 上启用 Fable 到 Opus 的回退行为。 |

启用魔法字符串缓存前，建议先阅读[提示缓存](/zh-cn/guides/claude-caching/)，特别是 OpenAI
对模型版本和 TTL 的要求。

Credential 行属于 provider。它包含 `kind`、密封后的 `secret_json`、`weight`、可选 `rpm_limit` / `tpm_limit`、可选代理和 TLS 覆盖，以及 `enabled`。密钥在 debug 输出中会被遮蔽，配置 master key 时会密封存储。

## Aggregated 与 Scoped 访问

GPROXY v2 支持两种访问上游的方式：

| 模式 | URL 形状 | 解析方式 |
| --- | --- | --- |
| Aggregated | `/v1/*`, `/v1beta/*` | 请求中的 `model` 通过 alias / route 表解析，再选择 route member 和 credential。 |
| Scoped | `/{provider}/v1/*`, `/{provider}/v1beta/*` | provider 名称来自路径；model 直接发往该 provider。 |

解析完成后，两种模式都进入同一套 classify、auth、transform、process、channel、settle 流程。Aggregated 是常规多上游网关模式；scoped 适合调试或临时暴露单个 provider。

## Routing Rules

Routing rule 是 provider 级配置。每一行包含：

- `operation`：例如 `generate_content`、`stream_generate_content`、`count_tokens`、`create_embedding`。
- `kind`：内容生成 wire kind，包括 `open_ai_responses`、`open_ai_chat_completions`、`claude_messages`、`gemini_generate_content`，或 provider kind `open_ai`、`claude`、`gemini`。
- `implementation`：`passthrough`、`transform_to`、`local` 或 `unsupported`。
- `transform_to` 可带 `dest_operation` 和 `dest_kind`。

没有匹配 routing rule 就是 `unsupported`。默认规则在创建 provider 时写入存储，console 可以从 channel 默认能力重置。

## Provider Rule Sets

把可复用 Rule Set 绑定到 Provider，可以添加系统指令、缓存断点、字段改写、文本替换或请求头。
规则在协议转换之后、请求发往上游之前执行。无效或不适用的规则会记录日志并跳过，不会让请求
失败。
