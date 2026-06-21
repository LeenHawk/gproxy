---
title: 模型、Route 与 Alias
description: 说明 v2 如何通过 alias、route、provider model、route member、variant 和 pricing 解析客户端模型名。
---

在 v2 中，客户端传入的 model name 不一定是上游 model id。Aggregated 流量会先把请求中的 `model` 解析为 alias 和 route，再选择 provider credential。

```text
request model
  -> alias_to_route
  -> route
  -> route_member
  -> provider + upstream_model_id
  -> provider credential
```

Scoped provider 流量会跳过 route 查找，因为 provider 已经来自 URL；但它仍会使用 provider model 目录来做模型列表、variant 去后缀、pricing 和可见性。

## Provider Models

`provider_models` 是每个 provider 的本地模型目录：

| 字段 | 含义 |
| --- | --- |
| `provider_id` | 所属 provider。 |
| `model_id` | 上游 model id。 |
| `display_name` | 可选展示名。 |
| `pricing_json` | 可选计费价格表。 |
| `variants_json` | 可选 suffix variant 暴露配置。 |
| `enabled` | 禁用模型不会暴露。 |

Console 可以通过 `/admin/providers/{provider_id}/upstream-models` 拉取实时上游模型列表。这个操作会调用 provider，或者在 channel 带静态目录时直接返回 bundled models。

## Routes 与 Members

Route 是 aggregated 模式下暴露给客户端的模型名。一个 route 可以有多个 member：

| 记录 | 关键字段 |
| --- | --- |
| `routes` | `name`、`strategy`、`enabled`、可选 `settings_json`。 |
| `route_members` | `provider_id`、`upstream_model_id`、`tier`、`weight`、`enabled`。 |
| `aliases` | `alias` -> `route_id`。 |

Snapshot 会按 `tier` 升序、`weight` 降序预排序 member。之后 balance 层根据 route strategy 和 provider credential strategy 做选择。

Alias 是多对一。如果 `chat-default` 指向 route `main-chat`，请求 `"model": "chat-default"` 会先解析到 `main-chat`。权限检查针对暴露的 route 或 provider 名称，而不是隐藏的 route member、credential 或上游 model id。

## 模型列表

模型列表端点属于 `Models` OperationGroup。入站 wire kind 由 endpoint 和凭据形式推断：

- OpenAI 和 Claude 共享 `/v1/models`；Claude 调用通过 `x-api-key` 识别，OpenAI 调用通过 `Authorization` 识别。
- Gemini 使用 `/v1beta/models`。
- `GET /v1/models/{id}` 和 `GET /v1beta/models/{id}` 会分类为 `get_model`。

Routing rule 决定 provider 对该 operation 使用 `local`、`passthrough`、`transform_to` 还是 `unsupported`。Local 模型列表从 snapshot 返回，并按当前用户权限过滤。

## Variants

`variants_json` 可以让一个 provider model 暴露多个 suffix variant。Snapshot 构建会把启用的 provider model 编译成：

- 用于模型列表响应的 exposed model list；
- 用于请求侧去 suffix 的 variant-to-base map。

适合用它表达上游支持、又希望客户端可见的模型后缀，而不是为每个展示 id 复制完整模型行。

## Pricing

价格保存在 `provider_models.pricing_json`，不是独立价格表。结算路径读取：

- `input`
- `output`
- `cache_read`
- `cache_creation`
- `image`

Token 价格是每百万 token。图片价格可以是每张图片的 flat value，也可以是按 `"{size}/{quality}"`、`"{size}"`、`"default"` 查找的 tiered object。缺失或格式错误的字段默认为 0：usage 仍会记录，但该调用不产生费用。
