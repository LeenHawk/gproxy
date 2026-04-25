---
title: 架构概览
description: GPROXY 工作空间结构 —— apps、crates 与 SDK 分层，以及一次请求是如何被处理的。
---

GPROXY 是一个 Cargo 工作空间，按三层组织：

1. **Apps** —— 可运行的二进制 (`gproxy`、`gproxy-recorder`)。
2. **Crates** —— 主程序组合使用的服务端组件 (`gproxy-core`、`gproxy-storage`、
   `gproxy-api`、`gproxy-server`)。
3. **SDK** —— 可在服务外独立复用的框架无关库 (`gproxy-protocol`、
   `gproxy-channel`、`gproxy-engine`、`gproxy-sdk`)。

## 工作空间布局

```text
gproxy/
├── apps/
│   ├── gproxy/              # 主二进制 (HTTP 服务 + 控制台宿主)
│   └── gproxy-recorder/     # 上游流量录制工具 (开发/调试)
├── crates/
│   ├── gproxy-core/         # 配置、身份、策略、配额、路由类型
│   ├── gproxy-storage/      # SeaORM 存储 + 加密 + schema 同步
│   ├── gproxy-api/          # 管理与用户 HTTP API、鉴权、登录、CORS
│   └── gproxy-server/       # 把上述部分组装在一起的 Axum 服务
├── sdk/
│   ├── gproxy-protocol/     # L0：OpenAI/Claude/Gemini 协议类型和 transform
│   ├── gproxy-channel/      # L1：Channel trait、各通道实现、凭证、计费、
│   │                        #     工具函数、健康状态
│   ├── gproxy-engine/       # L2：GproxyEngine、ProviderStore、路由、
│   │                        #     重试、凭证亲和性、后端 trait
│   └── gproxy-sdk/          # 重导出上述三层的伞 crate
├── frontend/console/        # React 控制台，构建时嵌入到二进制
└── docs/                    # 本文档站
```

## 请求生命周期

高层视角下，一个 LLM 请求会经过这些阶段：

```text
                ┌─────────────────────────────────────────────┐
HTTP 请求   ──► │  gproxy-server (Axum)                       │
                │    ├── 鉴权：API key → 用户身份              │
                │    ├── 协议分类 (OpenAI / Claude / Gemini)   │
                │    └── handler 分发                         │
                └───────────────┬─────────────────────────────┘
                                │
                                ▼
                ┌─────────────────────────────────────────────┐
                │  gproxy-engine :: routing                   │
                │    permission → rewrite → alias → execute   │
                └───────────────┬─────────────────────────────┘
                                │ 解析出的 (provider, model)
                                ▼
                ┌─────────────────────────────────────────────┐
                │  gproxy-engine :: GproxyEngine              │
                │    ├── channel.prepare_request(...)         │
                │    ├── 调用上游 HTTP                         │
                │    ├── 重试 + 健康状态更新                   │
                │    └── 用量记账                              │
                └───────────────┬─────────────────────────────┘
                                │
                                ▼
                          上游 LLM API
```

1. **鉴权。** 请求携带 API key (`Authorization: Bearer …`)。API 层解析为 `User`，
   检查用户与该 key 是否启用，并把身份附加到请求上下文。
2. **分类。** 路由层识别协议 (OpenAI Chat / OpenAI Responses / Claude / Gemini) 及
   路由类型 (`model_list` / `model_get` / chat 调用 …)。
3. **模型解析。** `permission → rewrite → alias → execute` 是唯一规范的顺序
   (详见*使用指南 → 模型与别名*)，别名、权限 glob 匹配、通道级重写规则都在这里生效。
4. **路由。** 对于 `*-only` 预设，`model_list` 和 `model_get` 直接由本地 `models`
   表响应，不会打到上游。对于 `*-like` / 透传预设，仍会调用上游，但响应会与本地
   `models` 表**合并**，让管理员注册过的条目始终可见。
5. **执行。** `GproxyEngine::execute` 让解析得到的通道准备上游请求，发起调用，
   处理重试，并更新每个凭证的健康状态。
6. **记账。** 用量通过 sink 异步写出，由后台 worker 批量写入存储。

## 后台 Worker

`apps/gproxy/src/workers/mod.rs` 拉起一组长期任务：

| Worker | 职责 |
| --- | --- |
| `UsageSink` | 消费用量消息并批量写入存储。 |
| `HealthBroadcaster` | 对每个凭证的健康状态变化做 debounce。 |
| `QuotaReconciler` | 周期性从实际用量重算 `cost_used`。 |
| `RateLimitGC` | 清理过期的限流计数器。 |

它们都参与优雅关机 —— 详见 [优雅关机](/zh-cn/reference/graceful-shutdown/)。

## 存储

`gproxy-storage` 基于 **SeaORM + SQLx**，支持 **SQLite**、**PostgreSQL** 和
**MySQL**。当设置了 `DATABASE_SECRET_KEY` 时，供应商凭证、用户密码和 API 密钥
会以 **XChaCha20-Poly1305** 加密后落盘。

内嵌控制台、管理 HTTP API 和 TOML 种子配置写入的是同一份 schema —— TOML
文件只在**首次**初始化 (数据库为空) 时被读取。详见
[TOML 配置](/zh-cn/reference/toml-config/)。

## SDK 边界

`sdk/` 下的 crate 刻意不依赖数据库、HTTP 服务和 Axum。这种隔离正是
`gproxy-sdk` 可以被嵌入到完全不同的应用里的原因。更多细节参见
[Rust SDK](/zh-cn/reference/sdk/)。
