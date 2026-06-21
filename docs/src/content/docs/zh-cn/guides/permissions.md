---
title: 权限、限速与配额
description: 跨 org、team、user scope 配置 route 访问权限、请求限速、token limit 和费用配额。
---

v2 的授权模型基于 scope。权限、限速和配额都可以挂在 org、team 或 user 上。

```text
user scope -> team scope -> org scope
```

Snapshot 按 `(scope, scope_id)` 保存匹配行，热路径不读取持久化层。

## Route Permissions

Route permission 用 glob pattern 授权 route 或 provider 名称：

```json
{
  "scope": "team",
  "scope_id": 42,
  "route_pattern": "chat-*"
}
```

有效权限是 user、team、org 三层 pattern 的并集。链路中任意 pattern 匹配暴露名称，请求即可继续；没有任何匹配则拒绝。禁用的 org 或 team 会拒绝请求，即使更低 scope 上有匹配 pattern。

Aggregated 模式下，权限匹配暴露的 route 名称；scoped 模式下匹配暴露的 provider 名称。它不会匹配隐藏的 route member、credential 或内部上游 model id。

## Rate Limits

Rate-limit 行也带 scope 和 route pattern：

| 字段 | 含义 |
| --- | --- |
| `rpm` | 每分钟请求数。 |
| `rpd` | 每天请求数。 |
| `total_tokens` | 按已结算 token counter 检查的每日 token 预算。 |

检查顺序是从最具体到最宽泛：user、team、org。第一个超限的匹配规则生效。

请求计数器使用 cache backend，并且先自增再判断是否超限。这样并发下行为确定，但被拒绝的请求也会消耗请求数预算。如果 counter backend 不可用，v2 对已配置限速 fail closed。

## Quotas

Quota 是某个 scope 的费用上限：

```json
{
  "scope": "org",
  "scope_id": 1,
  "quota_total": "100.00",
  "cost_used": "12.50"
}
```

用户链路上的每个 quota 都必须满足。Admission 会同时考虑持久化的 `cost_used` 和进行中的 pending spend。请求结算后，实际 usage 会回填 pending quota 并更新持久化费用。

价格来自 `provider_models.pricing_json`。没有配置价格的模型仍可运行并记录 usage，但费用为 0。

## 请求生命周期中的顺序

相关顺序是：

```text
auth user API key
  -> preprocess route/provider name
  -> permission and rate-limit admission
  -> estimate quota and pre-deduct pending spend
  -> balance, transform, process, channel
  -> settle actual usage and quota
```

因此授权看到的是 transform 前暴露给用户的 route/provider 名称，而不是 provider-native 请求体。
