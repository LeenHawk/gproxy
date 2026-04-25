---
title: 供应商与通道
description: GPROXY 中供应商、通道与凭证的组织方式。
---

GPROXY 里的**供应商 (provider)** 是一个具名的上游 LLM 端点。每个供应商绑定
恰好一个**通道 (channel)** —— 也就是负责说上游协议的那段代码 —— 并挂载
一到多个**凭证 (credential)**。

```text
Provider  ──(channel)──►  上游协议实现
   │
   ├── settings     (base_url、超时等)
   └── credentials  (API key、OAuth、service account…)
```

## 内置通道

`gproxy-channel` crate 中内置了以下通道。同名的 feature flag
(详见 [Rust SDK](/zh-cn/reference/sdk/)) 可用来只编译你需要的那部分。

| 通道 | 典型上游 | 备注 |
| --- | --- | --- |
| `openai` | api.openai.com、任意 OpenAI 兼容网关 | 若需要更自由地替换 base URL，可使用 `custom`。 |
| `anthropic` | api.anthropic.com | Claude Messages API。 |
| `aistudio` | Google AI Studio | `generativelanguage.googleapis.com`。 |
| `vertex` | Google Vertex AI | 通过 GCP service account 鉴权。 |
| `vertexexpress` | Vertex AI Express 模式 | 通过 API key 使用 Gemini 的简化方式。 |
| `geminicli` | 本地 Gemini CLI 工具 | 开发/桥接使用。 |
| `claudecode` | Anthropic Claude Code | 开发工具通道。 |
| `codex` | Codex 系列端点 | |
| `antigravity` | Antigravity agent runtime | |
| `deepseek` | api.deepseek.com | OpenAI 兼容的 DeepSeek。 |
| `groq` | api.groq.com | |
| `openrouter` | openrouter.ai | |
| `nvidia` | NVIDIA NIM 端点 | |
| `custom` | 任意 OpenAI 兼容上游 | 自建或第三方网关常用。 |

## 定义供应商

在种子 TOML 中：

```toml
[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [
  { api_key = "sk-upstream-1" },
  { api_key = "sk-upstream-2" }
]
```

或者在运行时，通过内嵌控制台的*供应商*标签页管理。`settings` 和每个
`credentials[i]` 都是自由形态的 JSON blob —— 具体 schema 取决于对应通道
(`OpenAiSettings`、`AnthropicCredential` 等)，控制台会为它们渲染结构化编辑器。

## 凭证与健康状态

当一个供应商挂着多张凭证时，GPROXY 会把它们当作一个轮转池。
`GproxyEngine` 每次选一个凭证发起上游调用，失败时更新**每个凭证独立的健康状态**
(限流的 key 进入冷却，失效的 key 被禁用等等)。`HealthBroadcaster` worker 会对这些
变化做 debounce，避免一次错误风暴冲击数据库。

可通过控制台或 `/admin/health` 端点查看当前状态。

## 两种路由模式

同一个 GPROXY 实例里，每个供应商都可以通过两种方式访问：

| 模式 | URL 形式 | 供应商从哪来 |
| --- | --- | --- |
| **聚合 (Aggregated)** | `/v1/...`、`/v1beta/...` | 来自 `model` 字段里的 `provider/model` 前缀（或命中别名）。 |
| **限定作用域 (Scoped)** | `/{provider}/v1/...` | 来自 URL 路径；`model` 字段里只放上游原始 id。 |

两种模式都覆盖全部协议（OpenAI / Claude / Gemini），都经过同一条
`permission → rewrite → alias → execute` 管道。按你的客户端挑一种即可 ——
具体的 curl 示例见 [发送第一个请求](/zh-cn/getting-started/first-request/)。

## 同协议透传

如果客户端和选中的上游使用**相同**协议 (比如一个 OpenAI 兼容的客户端打到
一个 OpenAI 供应商)，GPROXY 会以**最小解析**的方式转发请求 —— 鉴权、模型解析、
用量记账和限流照常生效，但请求 body 不做反序列化。这是热路径，也是 GPROXY
吞吐量的主要来源。

当协议不同时，`gproxy-protocol::transform` 层在入向转换请求形状，
出向再把响应转换回来。

## 延伸阅读

- [模型与别名](/zh-cn/guides/models/) —— 一个 `(provider, model)` 对是如何被命名和路由的。
- [TOML 配置参考](/zh-cn/reference/toml-config/) —— 种子文件支持的全部字段。
