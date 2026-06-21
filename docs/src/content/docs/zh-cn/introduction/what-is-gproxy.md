---
title: GPROXY v2 是什么?
description: GPROXY v2 重写版的高层介绍，以及它要解决的问题。
---

**GPROXY v2** 是 GPROXY LLM 网关的重写版。它延续 v1 的目标：用一个 HTTP
入口接入多个 LLM provider，并提供路由、凭证、用户 API key、策略、用量计费和浏览器控制台。
v2 的实现形态不同：它是一个 Rust crate，同时构建 native server binary 和 edge runtime
可用的 wasm library。

v2 的设计中心是 operation，而不是 provider family。协议行为按能力组织，例如模型列表、
token 计数、内容生成、embedding、image、compact 和 conversation。provider family 仍然在
wire boundary 上有意义，但不再是路由和 transform 的组织方式。

## v2 擅长什么

- **一个网关接多个 provider。** provider 是配置化的 channel instance，带 settings、
  credentials、health state、可选 TLS fingerprint 和 rule set。
- **OpenAI、Claude、Gemini 兼容流量。** v2 按 operation 和 wire kind 分类入口请求；
  同协议请求保持轻路径，跨协议请求转换成目标 provider 的 native 格式。
- **多租户访问控制。** user、org、team、user API key、route permission、rate limit 和
  quota 都属于控制面。
- **运行时路由。** 对外模型名可以解析到 route、route member、upstream model id 和
  credential；failover 与 health state 围绕这个执行路径工作。
- **native 与 edge 部署。** native binary 使用 Axum 和 wreq；wasm build 使用 fetch
  兼容 transport，并按平台能力接入 libSQL/Turso、Upstash 等后端。
- **内嵌管理控制台。** React console 单独构建，可以嵌入 native binary，也可以作为同源静态资源
  和 API 一起部署。

## 相比 v1 改了什么

v1 是 Cargo workspace，分 app crate、server crate 和 SDK crate。v2 把这些收敛到一个
crate，并在 `src/` 下保持清晰模块边界。这不是取消分层，而是打包形态变化：native binary、
wasm library 和共享 runtime 代码都在一个地方演进。

核心变化如下：

| 领域 | v1 形态 | v2 形态 |
| --- | --- | --- |
| 仓库结构 | apps、crates、SDK 组成的 workspace | 一个 crate，同时产出 native 和 wasm |
| 协议矩阵 | 更多地方按 provider family 描述 | 以 Operation / OperationGroup 为中心 |
| 配置流 | TOML/database 控制面 | import/export snapshot 加 persistence backend |
| Console | 单独 frontend，在 build 时嵌入 | React console 仍独立构建，同步到 `assets/console` |
| Edge | 不是主要运行时形态 | wasm library 和平台 bundle 是一等目标 |

## 核心概念

| 概念 | v2 中的含义 |
| --- | --- |
| Provider | 一个上游适配配置：channel id、settings、credentials、可选 proxy 和 TLS 行为。 |
| Channel | 准备 provider-native 请求并分类 provider-native 响应的代码。 |
| Operation | `GenerateContent`、`ListModels`、`CreateEmbedding`、`CountTokens` 等能力。 |
| Route | 对外公开的模型入口，选择一个或多个 provider/upstream model member。 |
| Alias | 映射到 route 的用户侧模型名。 |
| Rule set | protocol transform 之后、channel send 之前应用的有序请求改写规则。 |
| Snapshot | 请求热路径读取的控制面视图。 |
| Cache backend | session、counter、invalidation、lock 等临时/共享协调数据。 |
| Persistence backend | 控制面记录、日志、用量、审计和指标的持久真相源。 |

## 它不是什么

GPROXY v2 不是模型宿主，不运行推理；它也不是通用反向代理，而是理解 LLM protocol
operation 的网关。它也不是托管 SaaS 控制台；内嵌 console 属于你的部署，应放在自己的网络和
运维控制边界内。

## 下一步

- 读当前状态的[架构](/zh-cn/introduction/architecture/)。
- 按[安装](/zh-cn/getting-started/installation/)运行 v2。
- 在[快速开始](/zh-cn/getting-started/quick-start/)中导入本地开发快照。
- 用 [v1 到 v2 迁移](/zh-cn/deployment/v1-to-v2/)迁移已有 v1 部署。
