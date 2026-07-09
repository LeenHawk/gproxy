---
title: GPROXY v2 是什么?
description: 从实际使用角度了解 GPROXY 支持的客户端和它解决的问题。
---

**GPROXY v2** 是一个自托管的 LLM API 网关。应用只需要调用一个入口，GPROXY 会选择
上游 Provider，在 API 格式不同时转换请求和响应，执行访问与费用策略，并记录用量。内嵌
控制台把 Provider、凭据、模型、用户和日志集中在一个地方管理。

以下场景适合使用 GPROXY：

- 不希望应用代码绑定单一模型 Provider；
- 需要为现有客户端提供 OpenAI、Claude 或 Gemini 兼容接口；
- 希望一个公开模型名可以在多个上游账号或 Provider 之间路由；
- 需要为不同用户分配不同的模型权限、限额和预算；
- 希望在网关统一处理提示缓存、请求改写、故障转移和用量结算，而不是让每个应用重复实现。

## 一次请求如何处理

1. 客户端发送 OpenAI、Claude 或 Gemini 兼容请求。
2. GPROXY 校验 API key，并检查模型权限、限流和配额。
3. 请求中的模型名解析到具体 Provider、上游模型和可用凭据。
4. 上游 API 格式不同时，GPROXY 会转换请求与响应；Provider 规则和缓存断点在转换后应用。
5. GPROXY 记录用量、更新配额，再按客户端需要的格式返回响应。

请求可以走聚合 `/v1/...` API，由模型名选择路由；也可以走
`/{provider}/v1/...`，由 URL 指定 Provider。聚合路由适合应用日常调用，Scoped 路由则适合
Provider 专用客户端和故障排查。

## 核心概念

| 概念 | 含义 |
| --- | --- |
| Provider | 一个命名的上游连接，例如 OpenAI、Anthropic、OpenRouter、Vercel、Codex 或自定义端点。 |
| Credential | Provider 使用的 API key、OAuth token、Service Account 或会话。一个 Provider 可以配置凭据池。 |
| Model | 从 Provider 拉取或手动录入的上游模型，可以带价格信息。 |
| Route | 对外公开的模型入口，可以选择一个或多个 Provider/Model 成员。 |
| Alias | 解析到某条 Route 的另一个公开模型名。 |
| User API key | 客户端调用 GPROXY 时使用的密钥，拥有独立权限、限流和配额。 |
| Rule set | 可复用的请求规则，用于系统指令、缓存断点、JSON 改写、文本替换和请求头。 |

## 部署方式

- **原生二进制：** 适合 VM 或物理服务器，部署路径最直接。
- **Docker：** API 和控制台在同一容器中，适合大多数自托管场景。
- **Serverless Edge：** 在受支持的边缘平台运行，使用 Turso 保存持久配置，并可选接入
  Upstash 缓存。

三种方式提供相同的 API 和管理模型，但仍受平台能力限制。例如，Edge 部署不能像长驻服务器
那样使用本地 SQLite 文件或原生出站代理。

## 从 v1 升级

v2 仍然解决同一类问题，但配置和持久化模型已经变化。原生部署可以从现有 v1 SQLite
数据库导入受支持的控制面数据，并把旧文件保留为备份。替换线上实例前，请先阅读
[v1 到 v2 迁移指南](/zh-cn/deployment/v1-to-v2/)。

## GPROXY 不做什么

GPROXY 不托管模型，也不运行推理。它又不只是普通反向代理：它理解 LLM 请求格式、模型路由、
流式响应、工具调用、token 用量和不同 Provider 的鉴权方式。控制台属于你自己的部署，因此
网络暴露、备份和运维权限仍由你负责。

## 下一步

- [安装 GPROXY](/zh-cn/getting-started/installation/)或直接查看
  [快速开始](/zh-cn/getting-started/quick-start/)；
- 配置 [Provider 和凭据](/zh-cn/guides/providers/)；
- 管理[模型与别名](/zh-cn/guides/models/)；
- 添加[提示缓存断点](/zh-cn/guides/claude-caching/)；
- 需要了解实现细节时，再阅读[架构指南](/zh-cn/introduction/architecture/)。
