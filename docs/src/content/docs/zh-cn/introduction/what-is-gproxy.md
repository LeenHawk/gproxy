---
title: GPROXY 是什么?
description: 关于 GPROXY LLM 代理服务器及其设计目标的高层介绍。
---

**GPROXY** 是一个用 Rust 编写的高性能 LLM 代理服务器。它在多家上游供应商之上
暴露一个统一的、OpenAI / Anthropic / Gemini 兼容的 HTTP 接口，并额外提供
把它作为共享服务运行所需的一切基础设施：用户、API 密钥、模型权限、限流、
成本配额、用量记录，以及一个内嵌的浏览器控制台。

它以**单个静态二进制**的形式发布 (React 控制台已内嵌)，同时提供可选的
**Rust SDK** 供开发者在自己的应用中复用引擎。

## 它擅长做什么

- **单入口扇出到多上游。** 一个 GPROXY 实例可以把请求路由到
  OpenAI、Anthropic、Vertex / Gemini、DeepSeek、Groq、OpenRouter、NVIDIA、
  Claude Code、Codex、Antigravity、自定义 OpenAI 兼容端点等 —— 每一个都配置为
  独立的*供应商* (provider)。
- **多租户访问控制。** 为每个用户下发 API 密钥，用 glob 模型权限控制访问范围，
  按模型 pattern 施加 RPM / RPD / token 限流，按美元金额执行配额 —— 后台还有
  专门的协调任务保证数据自洽。
- **跨协议翻译。** 使用 OpenAI Chat Completions 的客户端可以路由到 Anthropic 或
  Gemini 上游 (反之亦然)，通过协议 `transform` 层完成请求和响应的格式转换。
- **同协议透传。** 当客户端与上游使用相同协议时，GPROXY 以最小解析开销转发字节，
  追求低延迟高吞吐。
- **可观测性。** 结构化的上下游请求日志 (body 可选)、按请求的用量统计、
  模型健康状态追踪 —— 全部在控制台中可视化。

## 它不是什么

- 它**不是模型宿主。** GPROXY 自己不做推理，只通过 HTTP 调用真正的上游供应商。
- 它**不是通用的 Web 负载均衡器。** 它理解 LLM 协议 (OpenAI / Claude / Gemini)
  并针对它们做了优化。
- 它**没有自带 SSO。** 内嵌控制台使用用户名 + 密码登录并签发 bearer session token；
  如果你需要 SSO，请把 GPROXY 放在反向代理之后。

## 核心概念速览

| 概念 | 在 GPROXY 中的含义 |
| --- | --- |
| **Provider (供应商)** | 一个已配置的上游 (名称 + channel + settings + credentials)。 |
| **Channel (通道)** | 某个上游协议的代码实现 (OpenAI / Anthropic / Gemini…)。 |
| **Model (模型)** | 供应商上可转发的具体模型 ID，可附带定价。 |
| **Alias (别名)** | 指向一个 `(provider, model)` 真实条目的友好名。 |
| **User (用户)** | 拥有一个或多个 API 密钥、权限、限流和配额的账号。 |
| **Permission (权限)** | `(user, provider, model_pattern)` 三元组。 |
| **Rate limit (限流)** | 按 `(user, model_pattern)` 作用的 RPM / RPD / token 上限。 |
| **Quota (配额)** | 按用户汇总的美元成本上限。 |

## 接下来

- **安装并运行** —— [安装](/zh-cn/getting-started/installation/)
- **5 分钟拉起可用配置** —— [快速开始](/zh-cn/getting-started/quick-start/)
- **理解内部结构** —— [架构概览](/zh-cn/introduction/architecture/)
