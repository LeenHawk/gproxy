---
title: GPROXY 是什么?
description: GPROXY 做什么、适合谁、一次请求如何流经它，以及控制台中会遇到的概念。
---

**GPROXY** 是一个自托管的 LLM API 网关。客户端只需要一个 Base URL 和一个 API
密钥。GPROXY 校验密钥，选择上游 Provider 与凭证，在客户端和上游线格式不同时转换
请求，执行你设定的规则和消费限制，并记录每个请求的成本。同一个二进制同时提供网
关、位于 `/` 的公开站点、位于 `/admin` 的运维控制台，以及位于 `/portal` 的用户门
户。

## 适合谁

- 持有多个上游账号、希望把它们放进一个池子、通过单一入口对外并具备故障转移和逐
  凭证健康跟踪的运维者。
- 希望应用代码只依赖一个模型名，而不是某一家厂商 SDK 的团队。
- 希望 Codex CLI 或 Claude Code 使用池化凭证、并且每个请求都被计量的用户。
- 需要在 LLM 流量前面加上按用户的权限、限流和成本配额的任何人。

## 接受的线格式

GPROXY 接受 OpenAI Chat Completions、OpenAI Responses、Claude Messages 和 Gemini
GenerateContent。任一格式的请求都可以由说另一种格式的上游来服务：六种成对转换全
部存在，缓冲响应和流式响应都支持。转换在两种格式之间直接进行，没有中间模式。流
以 SSE 交付，OpenAI Responses 也可以走 WebSocket，Gemini 则是增量 JSON 数组
（在 Gemini 路径上加 `?alt=sse` 选择 SSE）。

其他操作组走同一条路径：embeddings、图像、音频、视频、rerank、文件、Web 搜索、
token 计数、compaction、模型列表、realtime、memories 和 guardian。

## 一次请求如何处理

1. **入口与分类。** 方法和路径决定操作；第一段路径可以指定一个目标（见下文）。
   `stream` 标志或路径决定响应是否流式。
2. **鉴权。** 从 `Authorization: Bearer`、`x-api-key` 或 `x-goog-api-key` 读取 API
   密钥，解析为用户、团队和组织。
3. **路由。** 模型名按固定顺序预处理——别名、变体后缀、路由——得到一个有序候选列
   表：先按层级和权重排列路由成员，再从 Provider 的凭证池中取一个凭证。选择是确
   定性的轮转，绝不随机。
4. **准入。** 在密钥所继承的每一层作用域上检查权限、限流和成本配额。配额先按估算
   预扣。
5. **转换。** 如果上游使用另一种格式，先转换请求；然后 Provider 的规则集在 Provider
   原生请求上执行。
6. **上游。** 通道完成鉴权并发送请求。一次失败的尝试转到下一个候选，直到达到路由
   或实例的尝试上限（默认 6）。
7. **结算、捕获、遥测。** 响应转换回客户端格式，提取用量并计价，把配额的估算值调
   整为实际结算成本，并按日志设置记录这次交换。

所有到达上游的路径——SDK 调用、CLI 控制面、WebSocket——都经过同样的阶段，没有不
计量的捷径。

## 两种访问方式

| 模式 | 路径形状 | 由什么选择上游 |
| --- | --- | --- |
| 聚合 | `/v1/chat/completions`、`/v1/responses`、`/v1/messages`、`/v1beta/models/{model}:generateContent` | 模型名，经别名、变体和路由解析。 |
| 命名 | `/{target}/` 加原生路径，例如 `/codex/v1/responses` 或 `/codex/backend-api/codex/responses` | 第一段路径：一个 Provider 名、一个路由名，或某个公开模型的命名空间前缀，例如 `openai/...`。 |

目标是 Provider 名时，`model` 字段是上游模型 id，直接使用该 Provider 的凭证池；是
路由名时，使用该路由的成员；是命名空间时，`model` 是斜杠之后的部分。第一段路径
与任何目标都不匹配时，按聚合路径处理。

## 核心概念

| 概念 | 含义 |
| --- | --- |
| Provider | 基于某个通道建立的命名上游连接：Base URL 与端点覆盖、通道声明的类型化设置、采用 `round_robin` 或 `sticky` 策略的凭证池、可选的代理和 TLS 指纹。 |
| 凭证 | Provider 凭证池中的一个密钥：API key、OAuth token 对、会话 Cookie 或服务账号材料。带有权重、RPM/TPM 限制、代理与指纹覆盖和启用开关。健康状态按凭证和模型分别跟踪。 |
| 通道 | 面向某一上游家族的内置适配器：如何鉴权、有哪些路径、token 如何刷新、预置哪些路由默认值。二进制内置 28 个通道 id，从 `openai`、`claudeapi` 到 `codex`、`aistudio`、`aws-bedrock` 和 `custom`。 |
| 模型 | 记录在某个 Provider 下的上游模型，带显示名、上下文窗口、最大输出、思考能力标志和变体。可以从 Provider 拉取，也可以手动录入。 |
| 路由 | 对外公开的模型入口，成员各是一个 Provider 加一个上游模型，按层级（故障转移级别）和权重（同层内分流）排序。控制台里称为负载均衡。 |
| 别名 | 另一个传入模型名，在路由之前解析为目标名，可以是全局的，也可以只针对某个 Provider。 |
| 变体 | 公开模型的后缀形式，例如思考级别或 `-tier-*`，映射回基础模型并注入请求字段。 |
| 用户 API 密钥 | 客户端发送的密钥。属于某个用户，用户可以属于团队和组织。权限、限流和配额可以挂在任一层作用域上并向下继承。 |
| 规则集 | 可复用的有序规则列表，包含 `system_text`、`cache_breakpoint`、`rewrite`、`transform` 和 `header` 规则，作用于 Provider 原生请求与响应。挂在 Provider 上；创建 Provider 时会同时为它创建一个空的私有规则集。 |
| 路由规则 | 每个 Provider 上按操作和入站协议决定的处理方式：直通、转换到目标格式、本地应答，或拒绝为不支持。通道预置默认值。 |

## 部署方式

- **原生二进制。** 一个内嵌控制台的 `gproxy` 可执行文件，可以跑在服务器、桌面或
  手机上。提供 Linux、macOS、Windows 和 Android 安装包，以及同样目标的便携压缩包。
- **容器。** `ghcr.io/leenhawk/gproxy:<tag>`，同一个二进制的 `linux/amd64` 镜像，
  数据目录在 `/var/lib/gproxy`。
- **Edge wasm。** 面向 Cloudflare Workers、Deno Deploy 和 Netlify Edge 的预构建
  Bundle。配置保存在 libSQL，控制台由平台的静态层提供。
- **嵌入。** `gproxy-core` 是一个 Rust 库。其他应用可以链接它并调用与宿主相同的
  执行接口。这些 crate 未发布；嵌入意味着对本仓库的 path 或 git 依赖。

四种方式运行同一个核心和同一套管理模型。平台限制仍然存在：Edge 部署必须使用
libSQL，未配置共享缓存时只有单个 isolate，并且不提供 Claude Web 通道。

## 从 v2 升级

v3 延续了同样的职责和大部分词汇，但是一次重写。TOML 配置文件已经移除：配置由命令
行参数、`GPROXY_*` 环境变量和可选的 `.env` 文件组成。正在使用的 v2 SQLite 数据库
用 `gproxy migrate --from-v2 <path>` 导入，不加 `--apply` 时只做演练。把 v3 指向
v2 数据目录前，请先阅读 [v2 到 v3 迁移](/zh-cn/deployment/v2-to-v3/)。

## GPROXY 不做什么

GPROXY 不托管模型，也不运行推理。它也不是普通的反向代理：它解析 LLM 请求体、改写
流、提取 token 用量，并处理各 Provider 特有的鉴权。控制台和门户属于你的部署。
GPROXY 默认绑定 `127.0.0.1`；对外暴露、备份数据目录、保管主密钥都由你负责。

## 下一步

- [下载](/zh-cn/getting-started/downloads/)与[安装](/zh-cn/getting-started/installation/)。
- [快速开始](/zh-cn/getting-started/quick-start/)，配好第一条可用路由。
- [Provider 与凭证](/zh-cn/guides/providers/)和[模型、路由与别名](/zh-cn/guides/models/)。
- 需要请求生命周期细节时阅读[架构](/zh-cn/introduction/architecture/)。
