---
title: 架构概览
description: GPROXY v2 当前运行时架构和请求生命周期。
---

GPROXY v2 是一个 Rust crate，但有两个运行时出口：

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

## 稳定代码引用索引

部分源码注释沿用了最初 v2 设计笔记中未限定文档名的 `§` 标识。这些标识是稳定 ID，
不是本页标题的排列序号。本页调整结构时不要重新编号；应通过下表解析这些引用。
`RFC 7230 §6.1` 或带具体文档路径的引用仍指向对应的外部文档。

| 稳定标识 | 具名架构主题 |
| --- | --- |
| `§3.2` | 被动健康状态和熔断器。 |
| `§3.3` | 单 credential RPM/TPM 准入预算。 |
| `§4` | 共享 admin API contract 和 DTO。 |
| `§5` | 端到端请求生命周期和 pipeline 边界。 |
| `§6`、`§6.1` | Operation-first transform 和 provider rule processing。 |
| `§6.3` | Channel registry、本地 operation 和请求编排。 |
| `§6.4` | 上游 disposition 和有界 failover。 |
| `§7.2` | 控制面 snapshot、热重载和 invalidation。 |
| `§7.4` | 生效的上游 proxy、TLS fingerprint 和 HTTP transport。 |
| `§8` | 控制面 persistence 和 instance settings。 |
| `§8-A` | Route、route member、alias 和暴露的 provider model。 |
| `§8-B` | Provider、credential、model variant 和 transform dispatch。 |
| `§8-B2` | Routing rule 和 provider rule set。 |
| `§8-C` | Identity scope 下的 permission、rate limit 和 quota。 |
| `§8-D` | Usage record、wire log、capture 和 retention。 |
| `§8-E` | Runtime settings 以及 usage/log feature toggle。 |
| `§9` | Console build、内嵌和 edge asset 打包。 |
| `§13` | Cache 行为、invalidation 和 edge 配置刷新。 |
| `§14.1` | Secret envelope encryption 和使用时解密。 |
| `§14.2` | 首启 admin、密码 hash 和 session。 |
| `§14.3` | Secret redaction 和安全敏感的 runtime settings。 |
| `§14.5` | OAuth 登录、刷新和 credential usage 生命周期。 |
| `§15`、`§15.1`、`§15.2`、`§15.3` | 可观测性：request ID、tracing、metrics 和 latency。 |
| `§16.1`、`§16.2`、`§16.3` | Runtime 加固：优雅排空、过载/超时边界和健康状态持久化。 |
| `§17` | 标准化 usage、billing、quota 准入和 settlement。 |
| `§18` | 控制面 import 和 export。 |
| `§19` | Native self-update 生命周期。 |
| `§19.2`、`§19.3`、`§19.4` | 签名 manifest、release channel、回滚保护和更新策略。 |
| `§19.5`、`§19.6`、`§19.6.1`、`§19.6.2` | 下载/staging、binary swap、supervisor restart 和直接 re-exec。 |
| `§19.7`、`§19.8`、`§19.10` | 数据兼容、更新产物和 update admin/status 安全约束。 |

## 下一步

- 在[供应商与通道](/zh-cn/guides/providers/)中配置上游。
- 在[模型与别名](/zh-cn/guides/models/)中理解对外模型路由。
- 在[发行版构建](/zh-cn/deployment/release-build/)和
  [Edge Wasm](/zh-cn/deployment/edge/)中部署 native 与 edge build。
