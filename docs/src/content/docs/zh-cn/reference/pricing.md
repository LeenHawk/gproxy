---
title: 价格
description: GPROXY 如何给一个请求计价 —— token 成本、billing mode 变体，以及 admin 的编辑是怎样进入 billing engine 的。
---

GPROXY 处理的每个请求都会在响应阶段被计价，结果写入 `usages.cost` 列。本
页记录了价格的数据模型、价格数据的存储位置，以及 admin 编辑是如何传到
billing engine 的。

## `ModelPrice` 的形状

单个 JSON blob —— `models.pricing_json` —— 是一个
`(provider_id, model_id)` 行的权威价格源。它的形状和
`sdk/gproxy-channel/src/billing.rs` 里的
`gproxy_sdk::provider::billing::ModelPrice` 结构体完全一致：

```json
{
  "price_each_call": 0.005,
  "price_tiers": [
    {
      "input_tokens_up_to": 200000,
      "price_input_tokens": 3.0,
      "price_output_tokens": 15.0,
      "price_cache_read_input_tokens": 0.3,
      "price_cache_creation_input_tokens": 3.75,
      "price_cache_creation_input_tokens_5min": 3.75,
      "price_cache_creation_input_tokens_1h": 6.0
    }
  ],
  "flex_price_tiers": [],
  "scale_price_tiers": [],
  "priority_price_tiers": []
}
```

字段：

- `price_each_call` —— 每次请求收取的固定 USD 费用，不论 token 多少。
- `price_tiers[]` —— 按 `input_tokens_up_to` 分档的 per-token 价格。所有
  token 价格的单位是 **per 1,000,000 tokens**。选中的是第一个
  `input_tokens_up_to` ≥ 输入侧 token 总数（input + cache read +
  cache creation）的档位。
- `flex_price_each_call` / `flex_price_tiers` —— OpenAI `service_tier: "flex"`
  的覆盖。
- `scale_price_each_call` / `scale_price_tiers` —— `service_tier: "scale"`
  的覆盖。
- `priority_price_each_call` / `priority_price_tiers` —— OpenAI
  `service_tier: "priority"` 和 Anthropic `speed: "fast"` 的覆盖。

`model_id` 和 `display_name` 存在 `models` 表各自独立的列里，**不会**写进
JSON blob；它们在读取时会被重新盖到解析后的 `ModelPrice` 上。

## 价格数据存在哪里

- **内置 JSON** —— 每个 channel 都带一份默认价格表，路径在
  `sdk/gproxy-channel/src/channels/pricing/*.json`。这些 JSON 在编译时通过
  `include_str!` 嵌进二进制，在每个 provider 首次启动时被 seed 到 DB。
- **DB (`models.pricing_json`)** —— 运行时的权威数据源。admin 的编辑写到
  这里，bootstrap 只在行缺失时从内置 JSON seed。
- **内存中的 `MemoryModel.pricing`** —— 启动和每次 admin mutation 时，从
  DB 拷贝到 routing service 里的已解析 `ModelPrice`。
- **Billing engine** —— `ProviderInstance.model_pricing` 是 SDK
  `ProviderStore` 拥有的 `ArcSwap<Vec<ModelPrice>>`。它通过
  `engine.set_model_pricing(provider_name, prices)` 更新。

## Admin 编辑是如何到达 billing 的

当 admin 通过 console 或 `POST /admin/models/upsert` 写一个 model 时：

1. Handler 把 `pricing_json` 解析成 `ModelPrice` 验证合法性。非法 JSON
   在写 DB 之前就会以 `400 Bad Request` 被拒。
2. `storage.upsert_model(...)` 把行持久化到 DB。
3. `state.upsert_model_in_memory(...)` 把新的 `MemoryModel.pricing` 换进
   routing service。
4. `state.push_pricing_to_engine(provider_name)` 从内存快照里重建该
   provider 的 `Vec<ModelPrice>`，然后调用
   `engine.set_model_pricing(...)`。
5. 下一次 billing 调用（不管是当前请求还是别的并发请求）从 `ArcSwap` 里
   读到新价格。

admin 写和 billing 读之间**没有任何缓存层** —— push 是同步的，`ArcSwap`
的 swap 无锁。最后写入者胜出。

## Billing mode 选择

`BillingContext.mode` 是从请求体里推出来的：

| Channel           | 请求体里的信号                                 | Mode       |
|-------------------|------------------------------------------------|------------|
| `openai`          | `service_tier: "flex"`                         | `Flex`     |
| `openai`          | `service_tier: "scale"`                        | `Scale`    |
| `openai`          | `service_tier: "priority"`                     | `Priority` |
| `anthropic`       | `speed: "fast"`                                | `Priority` |
| `claudecode`      | `speed: "fast"`                                | `Priority` |
| 其他              | —                                              | `Default`  |

当 mode 不是 `Default` 时，引擎先在精确匹配的 model 上找对应模式的 tier
数组（`flex_price_tiers` 等），找不到就回退到 `default` 这个 model 行。
还找不到就回退到 `price_tiers`。

## Token 价格公式

对选中的 tier，每个非空的价格字段贡献：

```
amount = tokens × unit_price ÷ 1_000_000
```

在 `input_tokens`、`output_tokens`、`cache_read_input_tokens`、
`cache_creation_input_tokens`、`cache_creation_input_tokens_5min`、
`cache_creation_input_tokens_1h` 上累加。

tier 的选中使用的是 `effective_input_tokens(usage)`，即
`input + cache_read + cache_creation + cache_creation_5min + cache_creation_1h`
的总和。

## 价格匹配：精确 → `default` 回退

价格查找在 `model_id` 上做严格的字符串匹配：

1. 找一个 `model_id == request_model_id` 的精确匹配 `ModelPrice` 行。
2. 找不到就回退到 `model_id == "default"` 的行。
3. 两个都找不到，billing 返回 `None`，`usages.cost` 为 `0.0`。

**没有** regex、前缀匹配或 glob。想让多个 model 共享同一套价格档位，就
在 pricing JSON 里定义一个 `default` 行，让没有专属定价的 model 回退到
它。

## 遗留列

`models` 表上还有 `price_each_call` 和 `price_tiers_json` 列 —— 这是 v1
之前的老价格形状遗留。运行时**既不读也不写**它们；留着只是为了让
`backfill_legacy_pricing_json` 一次性迁移老部署的数据到第一次启动。后续
版本会通过显式 `ALTER TABLE` 把它们干掉。

## 价格有问题去哪里找

- **期望的价格没生效** —— 检查那一行的 `models.pricing_json` 是不是有
  数据。`pricing_json` 为 `NULL` 的行计费为 `0.0`。
- **Admin 改了没反应** —— 看服务端日志里有没有
  `push_pricing_to_engine: provider not registered in engine store` 这个
  warn。有就说明 admin 的写已经到 DB，但引擎的 provider store 里没有匹
  配的条目 —— 通常是 model 创建之后 provider 被重命名了。
- **选错了 tier** —— tier selector 用的是 `input_tokens + cache_* tokens`
  的总和，不是单独的 `input_tokens`。一个大部分 prompt 都命中了 cache 的
  请求也可能跨过 tier 边界，即便实际计费的 input 很小。
