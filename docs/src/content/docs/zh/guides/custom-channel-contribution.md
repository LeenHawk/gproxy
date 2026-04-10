---
title: 自定义渠道
description: 如何用 GPROXY 接入你自己的上游渠道，并理解这种模式的能力边界。
---

本页说明如何在不改代码的情况下接入你自己的上游渠道。

## 使用你自己的渠道（不改代码）

### 适用范围

这种方式适用于上游本身已经兼容**标准协议形态**：

- OpenAI 协议
- Claude 协议
- Gemini 协议

GPROXY 的 custom 适配器会按固定约定拼接标准路径（例如 `/v1/...`、`/v1beta/...`）和鉴权头（`Bearer`、`x-api-key`、`x-goog-api-key`）。

如果你的上游需要非常规签名、非标准鉴权握手、或者请求/响应结构改造很重，这种方式通常不够。

### 最小配置示例

```toml
[[channels]]
id = "mycustom"
enabled = true

[channels.settings]
base_url = "https://api.example.com"

[[channels.credentials]]
id = "mycustom-main"
label = "primary"
secret = "custom-provider-api-key"
```

### 可选 `mask_table`（请求体字段遮盖）

你可以在转发前删除请求体中的字段：

```toml
[channels.settings.mask_table]
rules = [
  { method = "POST", path = "/v1/chat/completions", remove_fields = ["metadata"] },
  { method = "POST", path = "/v1/responses", remove_fields = ["metadata"] },
]
```

`mask_table` 能做什么：

- 按方法和路径匹配（支持带 `*` 的前缀匹配）。
- 按 JSON 路径删除请求字段。

`mask_table` 不能做什么：

- 不能改响应体。
- 不能注入自定义签名逻辑。
- 不能定义任意新的协议转换实现。

### 这一模式的边界

custom 模式可以通过 `dispatch` 选择路由行为（`Passthrough` / `TransformTo` / `Local` / `Unsupported`），但仅限于 GPROXY **现有**的操作族和协议模型。

所以它非常适合快速接入标准兼容上游，但不适合引入全新线协议或高度定制转换链路。

如果你需要新增原生渠道实现，请看[开发与测试](/zh/reference/development/)中的“新增原生渠道贡献指南”。
