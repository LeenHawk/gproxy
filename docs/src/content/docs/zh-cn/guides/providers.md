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
构建。Claude 的 Cookie 登录仅在 native 可用，edge runtime 不会公布该登录模式。当前内置
channel id 包括：

| Channel id | 常见用途 |
| --- | --- |
| `openai`, `custom` | OpenAI API 或 OpenAI-compatible gateway。 |
| `azure` | Microsoft Foundry / Azure OpenAI；支持 OpenAI v1、Claude、嵌入、Compact 与 deployment-bound 图片接口。 |
| `aws-bedrock` | 使用 API key 接入 Amazon Bedrock 原生 control-plane 与 Runtime API。 |
| `openrouter`, `deepseek`, `groq`, `nvidia`, `vercel` | OpenAI-like 的 API-key provider。 |
| `claudeapi` | Anthropic Claude Messages API。 |
| `aistudio`, `vertex`, `vertexexpress` | Gemini / Vertex 上游；`vertex` 也支持原生 Claude 合作伙伴模型。 |
| `codex`, `claudecode`, `geminicli`, `antigravity`, `grokbuild`, `kiro`, `copilotcli` | OAuth、device-code、cookie 或 envelope 类型的 agent channel。 |
| `claudeweb` | 通过 claude.ai 会话 cookie 接入 Claude 消费版 web 后端（仅 native）。 |

上表描述官方 build。自定义 native binary 还可以链接外部 Channel crate。Console 会读取需要
鉴权的 runtime catalog，因此外部项会根据 `Channel::metadata()` 出现在 Provider selector 中，
不需要修改 Console 静态列表。编译时集成 contract 见
[添加 Channel](/zh-cn/guides/adding-a-channel/)。

成功返回的 runtime catalog 对当前可执行文件具有权威性，其中的凭据类型、登录模式、endpoint
类型、密钥模板与 usage 能力都以服务端为准；Console 只会为已知内置 Channel 叠加纯 UI 提示。
仅当确认旧版后端对 catalog endpoint 返回 404 或 405 时，Console 才使用静态列表说明已有的
内置 Provider。该回退仅用于显示：Provider 创建与保存、凭证手动创建与编辑、OAuth 和凭证批量
导入都会保持禁用；usage 查看以及不依赖元数据的删除和批量操作仍可使用。加载状态只出现在首次
成功取得 catalog 之前；后台重新获取期间仍可使用已缓存的权威元数据。其他真实错误会分别显示，
并可重试。Admin API 也会拒绝使用当前可执行文件未注册 Channel 的 Provider 创建或更新请求。

每个 channel 都声明 `(Operation, OperationKind) -> RoutingDecision` 的能力表。provider 的默认 `routing_rules` 由这张表生成。因此 v2 的协议能力按 Operation 组织，而不是按 OpenAI / Claude / Gemini provider 家族分桶。

### Azure 渠道

`azure` channel 使用 API key 凭据，`settings_json.base_url` 必须填写 Azure 资源根地址，例如
`https://<resource>.openai.azure.com` 或 portal 给出的 Foundry endpoint。OpenAI-family 请求会映射到
`/openai/v1/*` 并使用 `api-key` 请求头；Claude Messages 与 Count Tokens 会映射到
`/anthropic/v1/*` 并使用 `x-api-key`。请求中的模型 ID 必须是 Azure deployment 名称。

图片生成和编辑使用 Azure 当前的 deployment-bound 接口：
`/openai/deployments/{deployment}/images/generations` 与
`/openai/deployments/{deployment}/images/edits`。缺省 `api-version` 是
`2025-04-01-preview`，可通过 `settings_json.api_version` 修改，也可在精确 endpoint URL 中固定。
Azure Responses schema 已包含 `/openai/v1/responses/compact` 的 compaction 类型；若具体资源版本
尚未开放该接口，可用 `endpoints.openai_compact` 指向该资源支持的完整 URL。

当 OpenAI 与 Claude 部署使用不同资源域名时，在 `settings_json.endpoints` 中分别配置精确 URL；
精确 endpoint 优先于 `base_url`，并支持 `{model}` deployment 占位符。

Azure 同时支持两项提示词管理功能。Provider 的 `cache_breakpoint` 规则会给 OpenAI
Chat/Responses 插入原生 `prompt_cache_breakpoint`，或给 Claude Messages 插入
`cache_control`。可分别启用 `enable_openai_magic_cache` 和 `enable_claude_magic_cache`，让共用的
GPROXY 触发字符串只在对应目标协议中被删除，并在原位置插入原生缓存断点。Microsoft
Foundry 不提供 Anthropic 服务端回退，因此 Azure channel 不会注入 `fallbacks`。

### DeepSeek 渠道

`deepseek` channel 已支持 DeepSeek 原生 OpenAI-compatible Responses API，包括非流式与
HTTP/SSE 流式调用。客户端仍调用 GPROXY 标准的 `POST /v1/responses`；channel 会将其映射到
DeepSeek 官方的 `POST https://api.deepseek.com/responses`，并使用
`Authorization: Bearer` 鉴权。DeepSeek 官方只说明了 HTTP 流式接口，未说明 Responses
WebSocket transport，因此 GPROXY 会把 Responses WebSocket 客户端桥接到该 HTTP/SSE 接口。
仍可通过 `settings_json.endpoints.openai_responses` 配置精确 URL 覆盖。

DeepSeek 当前官方文档说明 Responses 仅支持 `deepseek-v4-flash`，`deepseek-v4-pro`
计划于 2026 年 8 月初支持。该接口无状态，不支持
`previous_response_id`、`conversation`、服务端存储与后台执行；支持文本/思维链、function
call、服务端联网搜索，以及 Codex 使用的 `apply_patch` custom tool，但不支持图片和文件输入。
其他不支持的 Responses 参数与 item 类型通常会被静默忽略。最新兼容表见 DeepSeek
[Responses 指南](https://api-docs.deepseek.com/zh-cn/guides/responses_api)。

在此能力加入前创建的 provider 会保留数据库中已有的路由规则。若要启用全部原生 Responses
与 WebSocket bridge route，请重置该 provider 的默认路由。

### Amazon Bedrock 渠道

`aws-bedrock` channel 使用 Amazon Bedrock API key，凭据保存为 `{"api_key":"..."}`。这里应
填写通常由 `AWS_BEARER_TOKEN_BEDROCK` 表示的 Bedrock bearer token，而不是 IAM Access Key
ID。所有上游请求统一使用 `Authorization: Bearer`；该渠道不使用 SigV4、Mantle，也不接入单独的
Claude Platform on AWS。

通过 `settings_json.region` 设置模型所在的 AWS 区域，缺省为 `us-east-1`。GPROXY 会生成
control-plane 地址 `https://bedrock.<region>.amazonaws.com` 和 Runtime 地址
`https://bedrock-runtime.<region>.amazonaws.com`。`control_base_url` 与 `base_url` 可分别覆盖这两个
根地址；精确 `endpoints` 的优先级更高。

模型列表与详情使用 `ListFoundationModels` 和 `GetFoundationModel`。OpenAI Chat Completions、
Responses、Claude Messages 与 Gemini 内容请求都会汇聚到 Runtime Converse。流式请求使用
ConverseStream，GPROXY 会增量解码 AWS EventStream 并输出下游协议要求的 SSE。
文本、图片、工具定义、工具选择、工具调用与工具结果都会映射到 Converse。流式工具参数按 content
block 缓冲，确保 Claude、OpenAI 和 Gemini 客户端都收到完整的参数对象。

OpenAI、Claude 与 Gemini Count Tokens 使用 Runtime `/model/{modelId}/count-tokens` 和原生
`input.converse` body；API key 的 IAM policy 必须允许 `bedrock:CountTokens`。OpenAI Compact 是
唯一不使用 Converse 的能力：AWS 官方不支持通过 Converse 压缩，因此改用 Runtime InvokeModel
的 Anthropic compaction。Compact route 应指向支持该能力的模型，例如
`us.anthropic.claude-sonnet-4-6`。

Claude `cache_control`、OpenAI `prompt_cache_breakpoint`、provider 缓存规则和已启用的 magic string
都会转换为 Converse `cachePoint`；Bedrock 的缓存读写 usage 再转换回下游协议。`cachePoint` 没有
Claude 1 小时或 OpenAI 30 分钟 TTL 的等价控制，因此使用 Runtime 模型的缺省缓存策略。

该 channel 不提供嵌入与图片操作；Converse 无法表达的 Responses hosted tool、后台任务与会话状态
也不受支持。

该渠道支持 provider `cache_breakpoint` 规则与魔法字符串缓存触发。Amazon Bedrock 不提供
Anthropic 服务端回退；请改用 provider 级路由或客户端回退。模型与 API 的可用性仍取决于区域。

### Claude 回退

在 `claudeapi`、`claudecode`、`vercel` 或兼容 Claude 的 `custom` channel 中设置
`settings_json.claude_fable_fallbacks`，可在 Claude 模型因策略拒绝时重试。该字段名为兼容已有
配置而保留。值为字符串
`"default"` 时使用 Anthropic 按拒绝类别维护的默认路由；也可填写一至三个按顺序尝试的模型
ID。GPROXY 会分别为默认路由和显式链加入 `server-side-fallback-2026-07-01` 或
`server-side-fallback-2026-06-01`。请求本身已有的 `fallbacks` 始终优先。对于已知不接受
Anthropic 服务端 `fallbacks` 参数的模型，GPROXY 会跳过该设置；未知及未来模型默认仍可使用。

OpenRouter 会把同一设置转换成自身的模型路由 `fallbacks` 数组，而不使用 Anthropic beta。
由于 OpenRouter 没有 Anthropic `"default"` 的等价模式，该值会转换成显式 Claude Opus 4.8
回退。

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
| `api_version` | `azure` 图片生成/编辑接口的 API 版本；缺省为 `2025-04-01-preview`。 |
| `region` | `aws-bedrock` 使用的 AWS 区域；缺省为 `us-east-1`。 |
| `enable_openai_magic_cache` | 在 OpenAI Chat/Responses 目标中识别 GPROXY 缓存触发字符串，并写入 OpenAI 显式断点。适用于 OpenAI、Azure、Amazon Bedrock、Codex、OpenRouter、Vercel 和 custom endpoint。 |
| `enable_claude_magic_cache` | 在 Claude Messages 目标中识别 GPROXY 缓存触发字符串，并写入 `cache_control`。适用于 Azure、Amazon Bedrock、Claude API、Claude Code、OpenRouter、Vercel 和 custom endpoint。 |
| `claude_fable_fallbacks` | 使用 Anthropic `"default"` 路由或一至三个有序模型重试 Claude 模型拒绝；适用于 Claude API 类 channel，并可作为 OpenRouter 显式模型链。 |

对于外部 Channel 表单，Console 的共用控件保留 `base_url`、`endpoints`、
`circuit_breaker` 和 `auto_refresh_models`。如果外部元数据也声明这些 key，由共用控件负责，
重复的通用字段会被忽略，从而确保每个 key 只渲染并序列化一次。这些控件会采用元数据默认值；
仅当没有已持久化值且 `endpoints`、`circuit_breaker` 的 object 默认值有效时才初始化该默认值。
字段标记为 required 时，`base_url` 不得为空，`endpoints` 必须至少包含一个有效的精确 URL，
`circuit_breaker` 的 `consecutive_failures` 与 `cooldown_secs` 都必须是正整数，满足这些条件后才能
保存 Provider。无法识别的已持久化设置会保留。其他外部设置声明使用通用控件；同一 key 重复声明
时采用第一个字段。

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
