---
title: 架构概览
description: GPROXY workspace 分层与请求运行机制。
---

## Workspace 结构

| 路径 | 职责 |
|---|---|
| `apps/gproxy` | 可执行服务入口（Axum + 内置前端静态资源） |
| `crates/gproxy-core` | AppState、路由编排、鉴权、请求执行 |
| `sdk/gproxy-provider` | 通道实现、重试、OAuth、dispatch、tokenizer |
| `crates/gproxy-middleware` | 协议转换中间件、usage 抽取 |
| `sdk/gproxy-protocol` | OpenAI/Claude/Gemini 类型与转换模型 |
| `crates/gproxy-storage` | SeaORM 存储层、查询模型、异步写队列 |
| `crates/gproxy-admin` | admin/user 业务域逻辑 |

## 启动阶段

服务启动时主要执行：

1. 读取 `gproxy.toml`，并应用 CLI/ENV 覆盖。
2. 建立数据库连接并自动同步 schema。
3. 初始化 provider registry、凭证池和凭证状态。
4. 确保 admin 用户（`id=0`）与 admin key 存在。

## 请求阶段

单次请求的核心链路：

1. 鉴权：解析并校验 `x-api-key`（或兼容头）。
2. 路由：按 scoped/unscoped + dispatch 决定目标 provider。
3. 凭证选择：从可用凭证中选择并执行失败重试/回退。
4. 协议转换：按 provider 协议规范进行转换或透传。
5. 记录：保存 upstream/downstream request 与 usage。

## 凭证健康状态

- `healthy`：可用。
- `partial`：部分可用，通常是模型级冷却。
- `dead`：不可用，暂不参与调度。

默认冷却行为：

- rate limit：约 `60s`
- transient failure：约 `15s`

这套状态机制的目标是降低单凭证抖动对整体成功率的影响。
