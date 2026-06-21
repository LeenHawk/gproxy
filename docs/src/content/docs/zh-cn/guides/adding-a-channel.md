---
title: 添加 Channel
description: 实现新的 v2 channel adapter，声明 Operation 路由能力，并注册到内置 registry。
---

Channel 是上游访问适配器。它负责注入认证、解析 endpoint URL、分类上游响应，并可选处理 provider quirk。它不负责跨协议 transform，也不负责 provider rule-set processing。

```text
transform/     按 Operation 做协议转换
process/       transform 之后执行 provider rule set
channel/       上游访问、认证、endpoint、response disposition
```

新增 channel 时要守住这个边界。

## 从相似 Channel 开始

Channel 位于 `src/channel/bulletins/`。优先复制最接近的已有 adapter：

| 上游形状 | 起点 |
| --- | --- |
| OpenAI-compatible API key | `openai`、`custom`、`deepseek`、`groq`、`nvidia` |
| Anthropic Messages | `claudeapi` |
| Gemini API key | `aistudio`、`vertexexpress` |
| Vertex service account | `vertex` |
| OAuth 或 agent envelope | `codex`、`claudecode`、`geminicli`、`antigravity`、`kiro`、`copilotcli` |

多数 channel folder 会把 auth、routes、shaping、OAuth 或 stream decoding 拆成小文件。跟随本地结构，不要把一个模块越写越大。

## 实现 `Channel`

必须实现的方法：

| 方法 | 责任 |
| --- | --- |
| `id()` | 稳定 registry id；必须匹配 `Provider.channel`。 |
| `provider_family()` | `open_ai`、`claude` 或 `gemini` family，用于 usage/billing context。 |
| `routing_table()` | 声明 `(Operation, OperationKind) -> RoutingDecision` 能力表。 |
| `prepare()` | 构建绝对上游请求并注入认证。 |

常用可选 hook：

| Hook | 何时使用 |
| --- | --- |
| `classify()` | 上游 status/body 需要 provider-specific retry、cooldown 或 auth-dead 处理。 |
| `shape_request()` | transform/process rule 后的 provider-native body 还需要 channel-local hygiene。 |
| `shape_response()` | 原始上游 body 在 response transform 前需要归一化。 |
| `stream_decoder()` | 上游 stream 是 envelope 或 binary，需要先解包成 SSE。 |
| `needs_refresh()` / `refresh()` | OAuth-like credential 需要使用前刷新。 |
| `prepare_usage_request()` / `parse_usage()` | provider 暴露 per-credential usage/quota endpoint。 |
| `default_emulation()` | native `wreq` 需要内置 TLS/HTTP2 impersonation profile。 |

`prepare()` 收到的是协议 transform 和 rule-set processing 之后的有效 body。不要在这里修改 body；channel-local 字段清理放在 `shape_request()`。

## 声明 Operation Routing

使用 `src/channel/routes.rs` 中的 helper：

```rust
use crate::channel::routes::{cg, pass, pv, xform};
use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

vec![
    pass(ListModels, pv(P::OpenAi)),
    xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
    pass(GenerateContent, cg(OpenAiChatCompletions)),
    xform(GenerateContent, cg(ClaudeMessages), GenerateContent, cg(OpenAiChatCompletions)),
]
```

Routing 必须 Operation-first。不要写 "OpenAI bucket" 或 "Claude bucket" 逻辑。每个 cell 单独说明该 channel 对这个 operation/kind 是 passthrough、transform、local 还是 unsupported。

创建 provider 时，route list 会写成存储里的 `routing_rules`。运行时 dispatch 读取存储规则；缺失行就是 unsupported。

## 注册 Channel

在 `src/channel/bulletins/mod.rs` 中加入模块，然后在 `src/channel/registry.rs` 的 `builtin_channels()` 中加入 channel。如果支持交互式登录，也要实现 `ChannelLogin` 并加入 `builtin_logins()`。

## 添加 Console Metadata

Console 需要足够的 metadata 来创建该 channel 的 provider 和 credential。检查 `console/src/lib/channel-meta.ts` 以及 provider / credential 表单。Provider-specific policy 优先做成 preset 或 UI helper；只有后端确实无法表达时，才增加 runtime primitive。
