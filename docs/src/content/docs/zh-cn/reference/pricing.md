---
title: Pricing
description: v2 如何存储模型价格、估算 quota admission 成本并结算最终 usage cost。
---

GPROXY v2 的 pricing 由独立的 `price_rules` 记录管理。规则可以限定到某个
provider (`provider_id`)，也可以是全局规则；模型匹配支持精确匹配和 substring
匹配。

pricing 和 quota 相关但不是同一层：

- pricing 描述某个 provider model 的单位价格；
- quota 描述某个 org、team 或 user 可以花多少钱。

没有匹配到启用价格规则时，请求仍然可以运行并记录 usage，但 cost 为 `0`。
格式错误的 decimal 价格字段会在写入或导入规则时被拒绝。

## Price rule 结构

```json
{
  "id": 1,
  "provider_id": 1,
  "match_type": "exact",
  "model_match": "gpt-4.1-mini",
  "input_price": "0.40",
  "output_price": "1.60",
  "cache_read_price": "0",
  "cache_creation_5m_price": "0",
  "cache_creation_30m_price": "0",
  "cache_creation_1h_price": "0",
  "image_output_price": "0",
  "enabled": true
}
```

`provider_id` 可以是 `null`。provider 为 null 表示全局规则。

## 价格字段

所有价格字段都是 decimal 字符串，并且都按每 1,000,000 tokens 计。

支持字段：

| 字段 | 含义 |
| --- | --- |
| `input_price` | 每百万 input token 价格。 |
| `output_price` | 每百万 output token 价格。 |
| `cache_read_price` | 每百万 cache-read token 价格。 |
| `cache_creation_5m_price` | 每百万 5 分钟 cache-creation token 价格。 |
| `cache_creation_30m_price` | 每百万 30 分钟 cache-creation token 价格。 |
| `cache_creation_1h_price` | 每百万 1 小时 cache-creation token 价格。 |
| `image_output_price` | 每百万生成图片 token 价格。 |

token cost 公式：

```text
cost =
  input_tokens * input_price / 1_000_000
+ output_tokens * output_price / 1_000_000
+ cache_read_tokens * cache_read_price / 1_000_000
+ cache_creation_5m_tokens * cache_creation_5m_price / 1_000_000
+ cache_creation_30m_tokens * cache_creation_30m_price / 1_000_000
+ cache_creation_1h_tokens * cache_creation_1h_price / 1_000_000
+ image_output_tokens * image_output_price / 1_000_000
```

## 图片价格

图片生成与其他可计费 operation 一样按 token 结算。`image_output_tokens` 与普通
`output_tokens` 互斥，因此文字输出与图片输出可以使用不同单价，且不会重复计费。
对于 OpenAI-compatible 响应，GPROXY 从
`completion_tokens_details.image_tokens` 读取图片 token 子集。专用图片 operation
如果只返回聚合 completion token，则这些 completion token 会视为图片输出 token。

价格规则不再提供按张计费字段。上游图片响应没有提供 token usage 时，GPROXY
会记录零 usage，无法据此计算本地费用。

## 运行时查找

control-plane snapshot 会缓存启用的 price rules。admission 和 settlement
时，GPROXY 会按 `(provider_id, upstream_model_id)` 解析价格。

规则匹配顺序是：

1. provider exact；
2. global exact；
3. provider contains；
4. global contains。

同一 rank 内，更长的 `model_match` 优先，最后用更小 `id` 保证结果确定。

## Admission 估算

发送上游请求前，quota admission 使用 best-effort 估算：

- 估算 input tokens 使用当前 pending-cost estimator 的请求 body length；
- output、cache 和 image-output 分量不做估算；
- 估算值按选中 price rule 的 token pricing 计价；
- 估算为 0 时跳过 pending quota 预扣。

对带 quota 的 scope，GPROXY 会把估算的 micro-dollar cost 加到
`qp:{scope}:{id}` cache key。这些 pending counter 有 15 分钟 TTL，因此
charge 和 refund 之间进程崩溃也会自愈。

## Settlement

成功的 content-generation response 会 exactly-once settle：

- 非流式和已完整 buffer 的响应 inline settle；
- native streaming response 会挂一个 guard；
- 正常 stream 结束记为 `Complete`；
- 上游中断或客户端断开会通过 guard 记为 `Interrupted`；
- 包装后的 stream 只会 settle 一次。

如果响应中有上游 usage，就直接使用；否则在编译 feature 支持时回退到本地计数。

settle 后会写入 `usages` 行，包含 token 数、usage source、结束状态、latency、route/provider/user 维度和 cost。quota reconcile 随后：

1. refund 精确的 pending micro-dollar 估算；
2. 按实际 settled cost 原子增加每个 quota-bearing scope 的 `quotas.cost_used`。

Embedding 和 image operation 有自己的 provider-shaped settlement 路径，二者都按
上游 token usage 结算。model list/get、token-count、compact 和 conversation
operation 当前不走 content-generation settlement 计费路径。

## 操作员在哪里改价格

使用 Console 的 Pricing 页面，或 price-rule admin endpoint：

```text
GET    /admin/price-rules
POST   /admin/price-rules
DELETE /admin/price-rules/{id}
```

JSON import/export 使用 `price_rules` 数组：

```json
{
  "price_rules": [
    {
      "id": 1,
      "provider_id": 1,
      "match_type": "exact",
      "model_match": "gpt-4.1-mini",
      "input_price": "0.40",
      "output_price": "1.60",
      "cache_read_price": "0",
      "cache_creation_5m_price": "0",
      "cache_creation_30m_price": "0",
      "cache_creation_1h_price": "0",
      "image_output_price": "0",
      "enabled": true
    }
  ]
}
```

admin mutation 后，GPROXY 会 invalidates control-plane snapshot，使新请求读取更新后的 price rules。
