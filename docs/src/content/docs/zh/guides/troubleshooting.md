---
title: 常见问题
description: GPROXY 常见报错与排查路径。
---

## 1) `401 unauthorized`

排查点：

- 请求是否携带 `x-api-key`（或兼容认证头）
- key 是否存在
- key 对应用户是否为 `enabled`

## 2) `403 forbidden`（admin 路由）

通常说明当前 key 不是 admin 用户（`id=0`）的 key。

## 3) `503 all eligible credentials exhausted`

常见原因：

- 通道没有可用凭证
- 凭证已被标记为 `dead`
- 目标模型处于 `partial` 冷却期
- 上游持续返回 429/5xx

## 4) `model must be prefixed as <provider>/...`

你在调用 unscoped 路由（例如 `/v1/chat/completions`）时，`model` 未使用 `<provider>/<model>` 前缀格式。

## 5) 实时 WebSocket 不可用

`/v1/realtime` 当前未实现，建议改用 `/v1/responses`（HTTP）。
