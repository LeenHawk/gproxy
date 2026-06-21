---
title: Provider 与 Channel
description: 在 GPROXY v2 中配置上游 Provider、凭据、Operation 路由能力、代理、TLS 指纹和 scoped 访问。
---

**Provider** 是一个命名的上游端点。它把稳定名称绑定到 channel 适配器、设置、凭据池、模型目录、路由规则和可选的请求处理规则集。

```text
Provider
|-- channel                 openai、claudeapi、aistudio、codex 等
|-- settings_json           base_url 和 channel 专用开关
|-- credentials             密封后的 API key、OAuth token、service account
|-- provider_models         本地模型目录和价格
|-- routing_rules           Operation + OperationKind 分发表
`-- provider_rule_sets      绑定到该 provider 的请求改写规则集
```

热路径从 `ControlPlaneSnapshot` 读取 provider。管理端修改会先写入持久化层，然后重建 snapshot 并广播失效；下一次请求即可看到新的控制面配置，不需要重启。

## 内置 Channel

native 和 edge runtime 构建同一套 channel registry。当前内置 channel id 包括：

| Channel id | 常见用途 |
| --- | --- |
| `openai`, `custom` | OpenAI API 或 OpenAI-compatible gateway。 |
| `openrouter`, `deepseek`, `groq`, `nvidia`, `vercel` | OpenAI-like 的 API-key provider。 |
| `claudeapi` | Anthropic Claude Messages API。 |
| `aistudio`, `vertex`, `vertexexpress` | Gemini / Vertex 上游。 |
| `codex`, `claudecode`, `geminicli`, `antigravity`, `kiro`, `copilotcli` | OAuth、device-code、cookie 或 envelope 类型的 agent channel。 |

每个 channel 都声明 `(Operation, OperationKind) -> RoutingDecision` 的能力表。provider 的默认 `routing_rules` 由这张表生成。因此 v2 的协议能力按 Operation 组织，而不是按 OpenAI / Claude / Gemini provider 家族分桶。

## Provider 字段

| 字段 | 含义 |
| --- | --- |
| `name` | 唯一 provider 名称；scoped 路由会在 URL 中使用它。 |
| `channel` | Channel registry id，例如 `openai` 或 `claudeapi`。 |
| `settings_json` | 自由 JSON 设置，常见字段包括 `base_url` 和 channel 开关。 |
| `credential_strategy` | 凭据池策略，目前是 `round_robin` 或 `sticky`。 |
| `proxy_url` | native 出站代理；edge 会忽略 native 代理设置。 |
| `tls_fingerprint` | provider 级 TLS/HTTP2 模拟配置；credential 可以覆盖。 |
| `enabled` | 禁用后不会参与路由。 |

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

可复用 rule set 通过 `provider_rule_sets` 绑定到 provider。绑定后的规则在 snapshot 重建时按顺序展开并编译，然后在协议 transform 之后、channel prepare 之前执行。当前的 system text、cache breakpoint、字段 rewrite、sanitize、header 都在这里运行。

当前后端保持宽松：无效规则会 warn 并跳过；provider 专用策略优先放在 console/config preset 中，除非 runtime 确实需要新的 primitive。
