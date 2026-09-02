---
title: "价格与分层"
description: "price_rules、price_rates 与 tiers_json，结算产出的指标，层级如何组合，准入与结算的关系，以及缺少用量时的估算"
---

定价在结算时回答一个问题：给定 Provider、上游模型和通道提取的规范化用量，
这次交换花了多少钱。它完全由数据驱动：一条 `price_rules` 行选中模型，其
`price_rates` 行为每个指标定价，其 `tiers_json` 按提示长度和服务层级调整
token 阶梯。配额（[权限、限流与配额](/zh-cn/guides/permissions/)）消费这个
结果。成本是不带货币的小数；所有价格共用你录入时的单位。

全新存储会加载内置的全局价格目录。在控制台 → 定价编辑价格，或使用
`/admin/api/price-rules`、`/admin/api/price-rates` 和
`POST /admin/api/default-model-catalog/apply-prices`。

## 定价规则

| 字段 | 含义 |
| --- | --- |
| `provider_id` | 作用范围。`null` 表示全局规则。 |
| `model_pattern` | 与上游模型 ID 匹配。`*` 匹配任意一段字符；其余按字面匹配。 |
| `tiers` | 下文描述的 `tiers_json` 数组，或 `null`。 |
| `priority` | 数值小者优先；相同时按 `id`。 |
| `enabled` | 停用的规则跳过。 |

对 `(Provider, upstream_model)` 的解析：先取限定到该 Provider 的第一条匹配
且启用的规则，否则取第一条匹配的全局规则。没有任何 `price_rates` 行的规则
会被整体忽略。没有匹配时请求照常运行、记录用量，成本为 `0`，日志中出现
`pricing missing; settling at zero cost`。

## 维度费率

| 字段 | 含义 |
| --- | --- |
| `rule_id` | 所属规则。 |
| `metric` | 用量指标名（见下文）。 |
| `unit_size` | 正整数。有效费率为每单位指标 `price / unit_size`。 |
| `price` | 非负小数字符串。 |
| `conditions` | 可选的 `维度: 值` 对象（字符串、数字或布尔标量）。 |
| `priority` | 同一指标多行之间的顺序；数值小者优先。 |

`input_tokens`、`output_tokens` 和 `cached_input_tokens` 行定义基础 token 阶
梯，按每百万费率读取（`price × 1,000,000 / unit_size`）。其他指标按
`数量 × price / unit_size` 计价。对同一指标，带条件的行按 `(priority, id)`
顺序尝试，条件全部等于结算维度的第一行生效；否则采用第一条无条件行，之后
的无条件重复行忽略。

```json
[
  { "rule_id": 1, "metric": "input_tokens", "unit_size": 1000000, "price": "0.40", "conditions": null, "priority": 0 },
  { "rule_id": 1, "metric": "output_tokens", "unit_size": 1000000, "price": "1.60", "conditions": null, "priority": 0 },
  { "rule_id": 1, "metric": "image_outputs", "unit_size": 1, "price": "0.04",
    "conditions": { "quality": "hd", "size": "1024x1024" }, "priority": 0 }
]
```

## 指标与维度

通道从每个响应中提取一个 `NormalizedUsage`：

```rust
pub struct NormalizedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub metrics: BTreeMap<String, Decimal>,
    pub dimensions: BTreeMap<String, String>,
}
```

三个 token 字段是 `usage_rows` 中的列；其余都是指标或维度。控制台目录已知
的指标名：

| 指标 | 单位 |
| --- | --- |
| `cache_creation_5m_tokens`、`cache_creation_30m_tokens`、`cache_creation_1h_tokens` | token，按每百万，受层级影响 |
| `image_output_tokens` | token，按每百万，受层级影响 |
| `reasoning_tokens`、`audio_input_tokens`、`cached_audio_input_tokens`、`audio_output_tokens`、`image_input_tokens`、`video_input_tokens`、`video_tokens` | token |
| `search_units`、`web_searches`、`web_fetches`、`image_outputs`、`video_outputs` | 计数 |
| `audio_seconds`、`video_seconds` | 秒 |

结算中观察到的维度是 `service_tier`（以及 `speed`）和图像操作的 `size`；
通道可以添加更多，任何维度都可以出现在 `conditions` 中。不在此列表中的指
标只要有费率指定它就会被计价；目录只是编辑器的便利，不是过滤器。
`cached_input_tokens` 以 `input_tokens` 为上限；未命中缓存的部分按输入费率计
价，缓存部分按缓存费率计价，没有缓存费率时回退到输入费率。

## 层级

`tiers_json` 是一个行数组。每行必须设置 `service_tier`、`min_prompt_tokens`
或两者。

| 字段 | 含义 |
| --- | --- |
| `service_tier` | 层级名。规范化为小写并把 `-` 替换为 `_`；`fast` → `priority`，`ultra_fast` → `ultrafast`，`default` 和 `on_demand` → `standard`。目录提供 `standard`、`priority`、`flex`、`scale`、`ultrafast`、`batch`、`reserved`；其他名称也接受。 |
| `min_prompt_tokens` | 提示 token（`input_tokens` 加全部 `cache_creation_*_tokens`）的阈值。默认 `0`。 |
| `multiplier` | 应用于该行未显式定价的分项的小数倍率。 |
| `input_price`、`output_price`、`cache_read_price`、`cache_creation_5m_price`、`cache_creation_30m_price`、`cache_creation_1h_price`、`image_output_price` | 该分项的显式每百万价格。 |

按分项组合：

1. **基础阶梯。** 在没有 `service_tier` 的行中，取 `min_prompt_tokens` 不超过
   提示长度的最高一行。它的显式价格替换其所设分项的基础费率。
2. **服务层级。** 层级取上游报告的值（`dimensions["speed"]` 或
   `dimensions["service_tier"]`），否则取请求要求的值（请求体中的 `speed`、
   `service_tier` 或 `serviceTier`）。在该层级的行中取已达到的最高阈值。
3. **价格。** 服务层级行中的显式价格优先。否则以基础阶梯价格乘以该行的
   `multiplier`（默认 1）。

这条规则有一个陷阱：显式层级价格会替换该分项的整个基础阶梯，包括它没有声
明的长上下文档位。基础输入 `1`，基础档位 `≥ 200,000 → 2`，提示 300,000
token：

| `batch` 行 | 有效输入费率 |
| --- | --- |
| `{"service_tier": "batch", "multiplier": "0.5"}` | `2 × 0.5 = 1` |
| `{"service_tier": "batch", "input_price": "0.5"}` | `0.5`——200k 档位丢失 |
| `{"service_tier": "batch", "min_prompt_tokens": 200000, "input_price": "1"}` | `1` |

显式层级价格要在它必须覆盖的每个阈值重复声明，或者改用倍率。控制台会标记
缺失的档位。

请求层级与实际层级：准入按请求要求的层级定价；结算从响应重新读取层级
（顶层，或 `usage`、`usageMetadata`、`response`、`message` 之下，或 Gemini
的 `x-gemini-service-tier` 头）并按其计费。被 Provider 降级为 `default` 的
`fast` 请求按 `standard` 行结算。

## 计算示例

来自 `crates/gproxy-core/src/tests/pricing.rs`：

- 基础输入 `1`、输出 `2`；基础档位 `≥ 100 → 输入 2` 和 `≥ 500,000 → 输入 3`。
  用量 1,000,000 输入和 1,000,000 输出。500,000 档位生效：
  `1 × 3 + 1 × 2 = 5`。
- 基础输入 `1`、输出 `2`、缓存 `0.5`；请求层级 `priority`；行
  `{min 1 → 输入 3}` 和 `{priority, min 2,000,000, 倍率 2, 输出 7, 图像输出 11}`；
  `image_output_tokens` 费率每百万 `4`。用量 2,000,000 输入（其中 1,000,000
  为缓存）、1,000,000 输出、1,000,000 图像输出 token。未缓存
  `1M × 3 × 2 = 6`，缓存 `1M × 0.5 × 2 = 1`，输出 `1M × 7 = 7`，图像
  `1M × 11 = 11`：合计 `25`。
- 行 `{priority → 输入 10}` 和 `{standard → 输入 4}`；请求写
  `"service_tier": "fast"`，响应头写 `default`。准入按每百万 `10` 估算；结算
  按 `4` 计费。

## 配额：先准入，后结算

准入只对可计费操作（结算模式非 free）运行。对计划中每个不同的
`(Provider, 上游模型)` 且有定价的候选，GPROXY 用分词器阶梯统计请求的输入
token，按请求层级定价；取候选中的最高成本，向上取整到微单位，作为估算值。
输出、缓存和维度指标不做估算。估算值加到调用方适用的每个配额的每个窗口
（总量、日、周、月、5 小时、7 天）的待结算计数器
（`gproxy:quota-pending:{window}`）上。若某个窗口在加入估算前已耗尽，或加
入后会超限，请求以 402 拒绝，并回滚已计入的金额。

结算时，实际成本按 `(请求, 窗口)` 写入 `quota_settlements` 和
`quota_windows.cost_used` 各一次，待结算估算值在同一个原子缓存操作中释放。
失败的请求释放估算值而不记录成本。成本以小数传递；只有待结算计数器使用整
数微单位。

## 缺少上游用量时的估算

token 统计（`gproxy-tokenize`）提取请求体中的文本，每条消息加 4 个 token，
然后依次尝试：GPT 系列模型的 tiktoken 编码；由 Provider 的 `tokenizer_map`
选出的 Hugging Face 词表，否则默认词表，否则与模型同名的词表（缺失的词表
在开启下载时会安排下载，本次继续向下回退）；内置的回退词表；最后是字符估
算 `ceil(字符数 / 2)`。准入和凭证 TPM 检查使用同一阶梯。

当可计费响应完全没有用量时，结算估算 `input_tokens = ceil(请求体字符数 / 2)`
和 `output_tokens = ceil(响应字符数 / 2)`（流式字节边经过边统计），记录
`usage_source = estimated`，并据此计价。没有用量的 `web_search` 响应仍计费
一次 `web_searches`。
