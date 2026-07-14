---
title: 模型、Route 与 Alias
description: 说明 v2 如何通过 alias、route、provider model、route member、variant 和 pricing 解析客户端模型名。
---

在 v2 中，客户端传入的 model name 不一定是上游 model id。Aggregated 流量会先把请求中的 `model` 解析为 alias 和 route，再选择 provider credential。

```text
request model
  -> global alias
  -> route 或 provider/model
  -> provider alias
  -> provider + upstream_model_id
  -> provider credential
```

Scoped provider 流量会跳过 route 查找，因为 provider 已经来自 URL；但它仍会使用 provider model 目录来做模型列表、variant 去后缀和可见性。

## Provider Models

`provider_models` 是每个 provider 的本地模型目录：

| 字段 | 含义 |
| --- | --- |
| `provider_id` | 所属 provider。 |
| `model_id` | 上游 model id。 |
| `display_name` | 可选展示名。 |
| `variants_json` | 可选 suffix variant 暴露配置。 |
| `enabled` | 禁用模型不会暴露。 |

Console 可以通过 `/admin/providers/{provider_id}/upstream-models` 拉取实时上游模型列表。这个操作会调用 provider，或者在 channel 带静态目录时直接返回 bundled models。

## Routes 与 Members

Route 是 aggregated 模式下暴露给客户端的模型名。一个 route 可以有多个 member：

| 记录 | 关键字段 |
| --- | --- |
| `routes` | `name`、`strategy`、`enabled`、可选 `settings_json`。 |
| `route_members` | `provider_id`、`upstream_model_id`、`tier`、`weight`、`enabled`。 |
| `aliases` | `provider`、正则 `alias`、替换目标 `target`、`sort_order`、`enabled`。 |

Snapshot 会按 `tier` 升序、`weight` 降序预排序 member。之后 balance 层根据 route strategy 和 provider credential strategy 做选择。

Alias 是按顺序执行的 full-match regex replacement。`provider="*"` 是全局规则，在 route/provider 解析前执行；provider-scoped alias 会在 provider 已知后执行。权限检查针对暴露的 route 或分层的 `provider/model` 名称，而不是隐藏的 route member、credential。

## 模型列表

模型列表端点属于 `Models` OperationGroup。入站 wire kind 由 endpoint 和凭据形式推断：

- OpenAI 和 Claude 共享 `/v1/models`；Claude 调用通过 `x-api-key` 识别，OpenAI 调用通过 `Authorization` 识别。
- Gemini 使用 `/v1beta/models`。
- `GET /v1/models/{id}` 和 `GET /v1beta/models/{id}` 会分类为 `get_model`。

Aggregated 与 scoped 模型列表使用相同策略。每个有权限的 provider 都会实时请求上游
（Aggregated 会并发请求），并使用独立超时。请求成功后只把尚不存在的模型追加到持久化
列表，绝不修改或删除已有行；超时或失败则使用累计的持久化列表。最终结果再按当前用户的
`provider/model` 权限逐条过滤。每个 provider 的超时固定为 10 秒。

## Variants

`variants_json` 可以让一个 provider model 暴露多个 suffix variant。Snapshot 构建会把启用的 provider model 编译成：

- 用于模型列表响应的 exposed model list；
- 用于请求侧去 suffix 的 variant-to-base map。

适合用它表达上游支持、又希望客户端可见的模型后缀，而不是为每个展示 id 复制完整模型行。

## Pricing

价格保存在独立的 `price_rules`，不再保存在 provider model 行上。规则可以限定到某个 provider，也可以是全局规则，并通过 `match_type` 和 `model_match` 匹配上游模型 id。

解析顺序是：

1. provider exact；
2. global exact；
3. provider contains；
4. global contains。

价格字段包括 `input_price`、`output_price`、`cache_read_price`、`cache_creation_5m_price`、`cache_creation_30m_price`、`cache_creation_1h_price` 和 `image_price`。Token 价格是每百万 token。图片价格是每张图片的 flat value。没有匹配规则时默认为 0：usage 仍会记录，但该调用不产生费用。
