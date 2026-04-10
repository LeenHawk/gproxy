---
title: 概述
description: GPROXY 的核心定位、能力边界与工程优势。
---

## GPROXY 是什么

`GPROXY` 是一个基于 Rust 的多通道 LLM 代理，目标是把不同上游（OpenAI、Anthropic、Google、以及自定义通道）统一暴露成一致的 API 网关。

## 核心能力

- 多通道与多凭证池管理，支持凭证健康状态与重试回退。
- 同时兼容 OpenAI / Claude / Gemini 风格请求。
- 支持 `ModelList / ModelGet / Generate / Stream / CountToken / Embedding / Compact` 等操作族。
- 内置 Admin Web 控制台，支持用户与密钥管理。
- 记录请求与 usage，便于审计与运营分析。

## 多数据库支持

存储层基于 SeaORM，当前编译特性已启用：

- `sqlx-sqlite`
- `sqlx-mysql`
- `sqlx-postgres`

也就是说你可以通过 `global.dsn` 直接切换后端数据库，而不需要改业务代码。

## 原生渠道与自定义渠道

内置原生渠道（12 个）：

- `openai`
- `claude`
- `aistudio`
- `vertexexpress`
- `vertex`
- `geminicli`
- `claudecode`
- `codex`
- `antigravity`
- `nvidia`
- `deepseek`
- `groq`

除内置渠道外，`ChannelId::Custom` 允许你声明任意自定义渠道（自定义 `id` + `base_url` + `credentials` + 可选 `dispatch`）。

## 协议转换能力

`dispatch` 是 GPROXY 的核心：每个渠道可定义“来源协议 -> 目标协议”的转换规则。

- 规则类型支持：`Passthrough / TransformTo / Local / Unsupported`
- 支持跨协议互转：OpenAI / OpenAI Chat Completion / Claude / Gemini / GeminiNDJson
- 支持按操作族路由，而不是仅按 URL 路由

这使得“上游只原生支持 A 协议，但你对外暴露 B/C 协议”成为可配置能力，而不是写死逻辑。

## 单二进制部署优势

管理端前端资源通过 `rust-embed` 编译进后端二进制：

- 不需要额外 Node.js 运行时
- 不需要额外 Nginx 静态站点服务
- 发布物可收敛为单二进制 + 配置文件
- 在边缘机器或最小化环境里部署更简单

### 交付形态

- 后端入口：`apps/gproxy/src/main.rs`
- 管理端资源嵌入：`apps/gproxy/src/web.rs`（`frontend/dist`）
- 生产部署可直接由单进程提供：
  - 管理后台（`/`）
  - 静态资源（`/assets/*`）
  - API 路由（`/admin/*`、`/user/*`、`/v1*`）

### 对运维的直接收益

- 组件更少：前后端不需要分别部署。
- 版本更稳：前端与后端天然同版本发布。
- 回滚更快：回滚单个二进制即可。
- 环境要求更低：对最小化主机或内网环境更友好。
