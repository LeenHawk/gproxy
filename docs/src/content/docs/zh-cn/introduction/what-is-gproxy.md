---
title: "GPROXY 是什么？"
description: "了解 GPROXY 的用途、支持的协议和常用概念。"
---


GPROXY 是一个可自行部署的大模型 API 网关。把上游账户接入 GPROXY 后，客户端只需配置网关地址、网关 API Key 和模型名。

它负责选择供应商和凭证，在需要时转换请求与响应格式，执行访问权限和限额，并记录用量与费用。GPROXY 不运行模型，实际推理由上游服务完成。

## 适合哪些场景

- 管理多个上游账户，希望集中配置凭证和故障切换。
- 给团队提供固定的模型名，更换供应商时不必修改每个客户端。
- 让 Codex CLI、Claude Code 等工具通过统一入口访问上游，并按用户记录用量。
- 在 Rust 应用中嵌入网关能力，见[嵌入核心库](/zh-cn/reference/embedding/)。

## 支持的协议

主要支持 OpenAI Chat Completions、OpenAI Responses、Claude Messages 和 Gemini GenerateContent，包括流式响应。协议不同时，GPROXY 在客户端和上游格式之间直接转换。

嵌入、重排、图像、音频、视频、文件和 Realtime 等操作也有对应接口，但可用范围取决于渠道、模型和部署方式。协议转换不能让上游获得它本身不支持的能力。接口清单见[路由与端点](/zh-cn/reference/routing-table/)。

## 常用概念

| 概念 | 含义 |
| --- | --- |
| 渠道（Channel） | 某类上游的适配器，规定登录方式、请求格式和可用操作，例如 `openai`、`codex`、`custom`。 |
| 供应商（Provider） | 保存的一条上游连接，包含名称、渠道、地址、配置和凭证池。 |
| 凭证（Credential） | 用于访问上游的 API Key、OAuth 令牌或 Cookie。同一个供应商可保存多份凭证。 |
| 模型路由（Route） | 客户端使用的模型名，对应一个或多个供应商与上游模型，可配置权重和回退层级。 |
| 网关 API Key | GPROXY 签发给客户端的密钥，关联用户、权限和费用预算。它不是上游密钥。 |
| 重写规则 | 修改系统提示词、缓存断点、JSON 字段、文本或请求头的规则。 |

凭证在配置主密钥时加密存储；CLI 未配置主密钥时使用明文存储并打印提示。详见[配置](/zh-cn/reference/configuration/)。

## 选择部署方式

| 方式 | 适合的用途 |
| --- | --- |
| Application | 在桌面或手机上使用，通过应用内向导和控制台管理。 |
| CLI / 容器 | 部署到服务器，提供 HTTP API 和浏览器控制台。 |
| Cloudflare Workers | 使用边缘运行环境和远程存储，支持 WebSocket / Realtime。 |

安装包和平台限制见[安装](/zh-cn/getting-started/installation/)。从 v3 更新时，请先阅读[迁移说明](/zh-cn/deployment/v3-to-v4/)。

## 模型名与请求地址

假设创建了 `fast` 路由，客户端可向 `/v1/chat/completions` 发送 `model: "fast"`。

也可以通过 `/openai-main/v1/chat/completions` 指定供应商，在请求中填写上游模型 ID。路由名含命名空间时，例如 `acme/fast`，可使用 `/acme/v1/chat/completions` 配合 `model: "fast"`。

解析顺序和命名限制见[模型与路由](/zh-cn/guides/models/)。第一次使用可以直接跟随[快速开始](/zh-cn/getting-started/quick-start/)。
