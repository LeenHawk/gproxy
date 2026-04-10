---
title: 概览
description: GPROXY 是什么、支持哪些功能、如何部署。
---

## GPROXY 是什么

GPROXY 是一个 Rust 编写的多通道 LLM 代理。它位于你的应用和上游 LLM 提供商之间，对外暴露统一的 API 网关，同时支持 OpenAI、Claude 和 Gemini 协议。你只需配置一次提供商和凭据，GPROXY 负责处理路由、协议转换、凭据池化、重试、故障转移、健康追踪和用量记录。

## 内置 Channel

GPROXY 内置 14 种 Channel 实现：

| Channel | 上游服务 |
|---|---|
| `openai` | OpenAI API |
| `anthropic` | Anthropic Claude API |
| `aistudio` | Google AI Studio |
| `vertexexpress` | Vertex AI Express |
| `vertex` | Vertex AI（服务账号） |
| `geminicli` | Gemini CLI / Code Assist |
| `claudecode` | Claude Code（基于 Cookie） |
| `codex` | OpenAI Codex CLI |
| `antigravity` | Antigravity / Code Assist |
| `nvidia` | NVIDIA NIM |
| `deepseek` | DeepSeek |
| `groq` | Groq |
| `openrouter` | OpenRouter |
| `custom` | 任意 OpenAI 兼容端点 |

`custom` Channel 可以指向任意 base URL 并使用任意凭据。设置 `id`、`base_url`、`credentials`，可选覆盖 dispatch 表。

## 协议 Dispatch

每个 Channel 都有一张 dispatch 表，将 `(OperationFamily, ProtocolKind)` 对映射到四种策略之一：

- **Passthrough** -- 原样转发请求。输入输出使用相同协议，解析开销最小。
- **TransformTo** -- 发送前将请求转换为另一种 `(operation, protocol)`。例如，OpenAI Chat Completions 请求打到 Anthropic 提供商时会被转换为 Claude Messages 格式。
- **Local** -- 不访问上游，直接在本地处理（如从静态表返回模型列表）。
- **Unsupported** -- 返回 501。

GPROXY 支持六种协议：

| 协议 | 传输格式 |
|---|---|
| `openai` | OpenAI Chat Completions（旧版格式） |
| `openai_chat_completions` | OpenAI Chat Completions |
| `openai_response` | OpenAI Responses API |
| `claude` | Anthropic Messages |
| `gemini` | Gemini GenerateContent |
| `gemini_ndjson` | Gemini 流式（NDJSON） |

跨协议转换自动完成。你可以向 Claude 提供商发送 OpenAI 格式的请求，也可以向 Gemini 提供商发送 Claude 格式的请求。GPROXY 会同时转换请求和响应。

## Operation 类型

路由按 Operation 分发，而非原始 URL 路径。Operation 类型包括：

`ModelList`, `ModelGet`, `GenerateContent`, `StreamGenerateContent`, `CountToken`, `Compact`, `Embedding`, `CreateImage`, `StreamCreateImage`, `CreateImageEdit`, `StreamCreateImageEdit`, `OpenAiResponseWebSocket`, `GeminiLive`, `FileUpload`, `FileList`, `FileGet`, `FileContent`, `FileDelete`

## 多数据库支持

存储层使用 SeaORM，编译了三个数据库驱动：

- **SQLite** -- 默认，零配置。首次运行时 GPROXY 自动创建数据库文件。
- **MySQL** -- 设置 `--dsn mysql://user:pass@host/db`
- **PostgreSQL** -- 设置 `--dsn postgres://user:pass@host/db`

Schema 在启动时自动同步。切换数据库只需修改 DSN。

## 单二进制部署

React 管理控制台（Vite + Tailwind 构建）通过 `rust-embed` 编译进二进制文件。一个进程提供所有服务：

- 控制台 UI 位于 `/console/*`
- API 路由位于 `/v1/*`、`/admin/*`、`/user/*`
- 提供商路由位于 `/<provider>/v1/*`

根路径 `/` 重定向到 `/console/login`。

无需 Nginx，无需 Node.js 运行时，无需单独部署前端。只需发布一个二进制文件，可选附带 `gproxy.toml`。

## 项目结构

```
gproxy/
  apps/gproxy/          -- 二进制入口，Axum 服务器，嵌入式控制台
  sdk/gproxy-protocol/  -- OpenAI/Claude/Gemini 类型定义及跨协议转换
  sdk/gproxy-provider/  -- Channel trait，dispatch 表，凭据重试，计费
  sdk/gproxy-routing/   -- 路由分类，模型别名，权限，速率限制
  sdk/gproxy-sdk/       -- 顶层 SDK 重导出
  crates/gproxy-core/   -- 领域服务（配置、身份、策略、配额、路由、文件）
  crates/gproxy-server/ -- AppState、中间件、会话管理
  crates/gproxy-api/    -- HTTP 处理器（admin、user、provider），引导，登录
  crates/gproxy-storage/-- SeaORM 实体，仓储实现，异步写入 sink
  frontend/console/     -- React 管理控制台源码（@gproxy/console）
```
