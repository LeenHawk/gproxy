---
title: Pricing
description: v2 如何存储模型价格、估算 quota admission 成本并结算最终 usage cost。
---

gproxy v2 的 pricing 属于 provider model。权威配置是
`provider_models.pricing_json`；当前没有单独的价格表。

pricing 和 quota 相关但不是同一层：

- pricing 描述某个 provider model 的单位价格；
- quota 描述某个 org、team 或 user 可以花多少钱。

未配置价格的模型仍然可以运行。缺失、null 或格式错误的 pricing 字段会被解析为 0，因此 usage 会记录，但 cost 为 `0`。

## `pricing_json` 结构

token 价格按每 1,000,000 tokens 计。推荐使用字符串 decimal，因为金额用
decimal 运算；JSON number 也会被接受。

```json
{
  "input": "3.00",
  "output": "15.00",
  "cache_read": "0.30",
  "cache_creation": "3.75"
}
```

支持字段：

| 字段 | 含义 |
| --- | --- |
| `input` | 每百万 input token 价格。 |
| `output` | 每百万 output token 价格。 |
| `cache_read` | 每百万 cache-read token 价格。 |
| `cache_creation` | 每百万 cache-creation token 价格。 |
| `image` | 图片 operation 的每张图价格；可以是 flat scalar，也可以是 tier object。 |

token cost 公式：

```text
cost =
  input_tokens * input / 1_000_000
+ output_tokens * output / 1_000_000
+ cache_read_tokens * cache_read / 1_000_000
+ cache_creation_tokens * cache_creation / 1_000_000
```

## 图片价格

图片 operation 的 `image` 可以是每张图 flat price：

```json
{ "image": "0.04" }
```

也可以是 tier object。查找顺序：

1. `"{size}/{quality}"`；
2. `"{size}"`；
3. `"default"`；
4. 没有匹配则为 0。

```json
{
  "image": {
    "1024x1024": "0.04",
    "1792x1024/hd": "0.12",
    "default": "0.02"
  }
}
```

图片价格按生成图片数量计，不是按百万 tokens 计。

## 运行时查找

control-plane snapshot 会按 provider id 缓存 provider models。admission
和 settlement 时，gproxy 用 `(provider_id, upstream_model_id)` 精确查找
对应 model，并解析该 model 的 `pricing_json`。

当前 v2 pricing lookup 没有 glob、prefix 或 `"default"` model fallback。
需要产生非零费用的 provider model 行都应单独配置 pricing。

## Admission 估算

发送上游请求前，quota admission 使用 best-effort 估算：

- 估算 input tokens 使用当前 pending-cost estimator 的请求 body length；
- output、cache 和 image 分量不做估算；
- 估算值按选中 provider model 的 token pricing 计价；
- 估算为 0 时跳过 pending quota 预扣。

对带 quota 的 scope，gproxy 会把估算的 micro-dollar cost 加到
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

Embedding 和 image operation 有自己的 provider-shaped settlement 路径。
model list/get、token-count、compact 和 conversation operation 当前不走
content-generation settlement 计费路径。

## 操作员在哪里改价格

使用 console 或 provider-model admin endpoint：

```text
GET  /admin/providers/{provider_id}/models
POST /admin/providers/{provider_id}/models
```

JSON import/export 使用同样的 `provider_models` input shape：

```json
{
  "id": 1,
  "provider_id": 1,
  "model_id": "gpt-4.1-mini",
  "display_name": "GPT-4.1 mini",
  "pricing_json": {
    "input": "0.40",
    "output": "1.60"
  },
  "variants_json": null,
  "enabled": true
}
```

admin mutation 后，gproxy 会 invalidates control-plane snapshot，使新请求读取更新后的 model 和 pricing 行。
