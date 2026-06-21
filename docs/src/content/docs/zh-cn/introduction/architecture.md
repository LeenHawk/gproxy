---
title: 架构概览
description: gproxy v2 当前运行时架构和请求生命周期。
---

gproxy v2 是一个 Rust crate，但有两个运行时出口：

- `src/main.rs` 中的 native binary，由 Axum 和 native upstream client 提供服务；
- `src/lib.rs` / `src/http/edge/` 中的 wasm library entry，用于 edge 平台 bundle。

它仍然是分层设计。和 v1 的区别是打包方式，不是工程纪律：v2 在一个仓库里继续分离
protocol type、transform、请求编排、channel、storage、admin 和部署边界。

## 仓库布局

```text
.
|-- Cargo.toml              # 一个 crate：lib + bin
|-- src/
|   |-- main.rs             # native CLI、config、AppState、Axum server
|   |-- lib.rs              # shared module surface 和 wasm export
|   |-- app/                # bootstrap、snapshot、import/export、v1 migration
|   |-- protocol/           # Operation taxonomy 和 provider wire model
|   |-- transform/          # 按 operation 组织的协议转换
|   |-- process/            # provider rule-set 编译与应用
|   |-- channel/            # 上游适配器和 registry
|   |-- pipeline/           # 请求生命周期编排
|   |-- http/               # native server、edge adapter、admin API dispatcher
|   |-- store/              # cache 和 persistence backend
|   `-- admin/ billing/ credentials/ health/ tokenize/ selfupdate/ usage/
|-- console/                # React 控制台，独立构建
|-- assets/console/         # 生成的 console embed 目标
|-- deploy/                 # edge 和平台打包入口
|-- docs/                   # Starlight 文档网站
`-- dev-docs/               # 开发者/source 笔记，用作参考材料
```

## 请求生命周期

一次常规生成请求经过：

```text
HTTP request
  -> classify operation and inbound wire kind
  -> authenticate user API key
  -> normalize model name and alias
  -> resolve route or scoped provider
  -> enforce route permissions, rate limits, and quota admission
  -> select route member and credential
  -> transform protocol if inbound and upstream wire kinds differ
  -> apply provider rule sets
  -> prepare upstream request in channel
  -> send request through native or fetch client
  -> classify provider response
  -> fail over or settle usage
  -> shape response and transform back if needed
  -> log request, usage, quota deltas, and health state
```

`pipeline::execute` 是中心编排器。它把分类、认证、预处理、路由解析、鉴权、balance、
transform、failover 和 settle 分给小模块处理。

## Operation-first 协议模型

v2 不把 provider family 当成主要文档和代码模型。中心概念是：

| 类型 | 作用 |
| --- | --- |
| `OperationGroup` | 大类能力：models、count tokens、generate content、images、embeddings、compact、conversation。 |
| `Operation` | 具体动作，例如 `ListModels`、`GenerateContent`、`CreateEmbedding`、`CompactContent`。 |
| `OperationKind` | 这个 operation 的 provider wire shape，例如 OpenAI Responses 或 Claude Messages。 |
| `OperationKey` | `(operation, kind)`，被 routing rule 和 transform 使用。 |

因此 content generation 下有多个 OpenAI kind：OpenAI Responses 和 Chat Completions
是不同的 native wire shape，不只是两个名字。

## Transform、Process、Channel

三层必须分开：

- **Transform** 按 operation 改协议形状。route 执行需要时，它在 OpenAI、Claude、Gemini
  wire model 之间转换。
- **Process** 在 transform 之后、channel 看到请求之前应用配置化请求改写规则。engine 应保持宽松；
  provider-specific preset 应优先放在配置和 console 里，除非 runtime 真正需要新 primitive。
- **Channel** 负责上游访问：endpoint、auth、request prepare、response disposition、可选 stream
  decode、OAuth refresh、usage endpoint 和 native TLS/HTTP2 profile。

## AppState 与快照

每个请求拿到一个轻量 clone 的 `AppState`。热路径读取
`ArcSwap<ControlPlaneSnapshot>`，其中包含 provider、route、rule 和 identity 记录。
控制面写入会更新 persistence，重建本地 snapshot，并在 cache backend 支持时发布 invalidation。

native 实例可使用 memory/Redis cache 与 file/db persistence。edge 实例使用 fetch-compatible
client，以及 libSQL/Turso、REST 风格共享存储等平台友好的 persistence/cache backend。

## 运行时边界

| 运行时 | 边界 |
| --- | --- |
| Native | CLI/env config、Axum server、内嵌 console assets、native wreq client pool、可选 self-update。 |
| Edge | wasm entry、fetch adapter、平台环境；默认不嵌入 console binary assets。 |
| Console | `console/` 中的 React SPA；构建产物同步到 `assets/console/` 给 native embedding。 |
| Documentation | `docs/` 中的 Starlight 站点；开发/source 笔记放在 `dev-docs/`。 |

## 下一步

- 在[供应商与通道](/zh-cn/guides/providers/)中配置上游。
- 在[模型与别名](/zh-cn/guides/models/)中理解对外模型路由。
- 在[发行版构建](/zh-cn/deployment/release-build/)和
  [Edge Wasm](/zh-cn/deployment/edge/)中部署 native 与 edge build。
