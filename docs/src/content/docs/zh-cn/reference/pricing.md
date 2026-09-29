---
title: "价格与分层"
description: GPROXY v4 怎么给一次交换定价：规则选择、费率行及其条件、上下文与服务档位阶梯，以及完全没有价格时会怎样。
---

计价在结算时回答一个问题：**这次交换花了多少**——给定 Provider、上游模型，以及渠道提取出
的归一化用量。

它发生在**引擎内部**、在结算时，因此预算和观察者看到的是同一个数。v3 由引擎把用量交上去、
再由应用层算一遍，于是产生了两套时间语义、两套模型匹配，以及两个"没有价格怎么办"的答案。
v4 每样只有一个。

三张表，一种形状：一行 `price_rules` 选中一个模型，它的 `price_rates` 行给每个指标定价，
它的 `price_tiers` 行按提示长度和服务档位调整 token 阶梯。

## 规则选择

一条规则适用，当

- 它的 provider 是这次交换的 provider，**或未设**（一条全局规则）；
- 它的 `modelPattern` glob 匹配**上游**模型名；
- 它的 `operation` 是这次请求的，**或未设**（覆盖全部操作）。

**Provider 规则先于全局规则。** 同一个作用域内 `(priority, id)` 最小者胜。

| 字段 | 含义 |
| --- | --- |
| `providerId` | `null` 是全局规则 |
| `modelPattern` | 对上游模型名的 `*` / `?` glob |
| `operation` | `null` 覆盖该模型的每个操作 |
| `priority` | 越小越先；id 破平 |
| `currency` | 固定为 `USD`，所有价格和结算金额统一使用美元 |
| `enabled` | |

### 一条规则都没有

这次交换是**未定价**的：不花钱，照样结算，而且它的用量带着

```json
{"dimensions": {"unpriced": "true"}}
```

这是一个信号，不是一个拒绝。运维者想知道有个模型在被白嫖；他们不想因为还没人填价格就
被拒掉一个请求。

## 费率行

一个费率行是某个指标的基础价。

| 字段 | 含义 |
| --- | --- |
| `metric` | 一个内置键，或渠道产出的任意自定义键 |
| `unit` | `token`、`count`、`second` 或 `character` |
| `unitQuantity` | 正的分母：1 000 000 个 token、1 张图、60 秒 |
| `value` | `unitQuantity` 个单位的非负价格，币种取自父规则 |
| `conditions` | `null` 是兜底行；否则是一个非空的"维度名 → 标量"对象，**全部**条件都要匹配 |
| `priority` | 同一指标的多行中越小越先 |

对一个指标，带条件的行按 `(priority, id)` 顺序尝试，第一个条件全部匹配结算维度的胜出；
否则兜底行生效。**被选中的条件费率*替换*基础费率**，不是与它复合。

```json
[{"metric":"input_tokens",  "unit":"token","unitQuantity":"1000000","value":"0.40"},
 {"metric":"output_tokens", "unit":"token","unitQuantity":"1000000","value":"1.60"},
 {"metric":"image_outputs", "unit":"count","unitQuantity":"1","value":"0.04",
  "conditions":{"quality":"hd","size":"1024x1024"},"priority":0}]
```

### 阻止重复计费的那条规则

**费率必须消费互不相交的量。** 命中缓存的 input 要从普通 input 里扣掉；已经通过聚合项
计费的 reasoning 或媒体 token 子集不能再收一次。按图计费和按 token 计费是*互斥*的基准，
除非厂商真的两样都收。

一个通用的 `tool_calls` 行和一个具体的 `web_searches` 行不能给同一次调用都计费。而客户端
*声明*了一个工具，并不构成一次可计费的服务端执行。

## 指标

**token**——习惯上按每百万计：

```text
input_tokens          output_tokens           cached_input_tokens
cache_creation_5m_tokens  cache_creation_30m_tokens  cache_creation_1h_tokens
reasoning_tokens      image_input_tokens      image_output_tokens
audio_input_tokens    cached_audio_input_tokens  audio_output_tokens
video_input_tokens    video_tokens
```

**计数、秒与字符：**

```text
image_outputs   video_outputs   audio_seconds   video_seconds   audio_characters
search_units    web_searches    web_fetches     file_searches
code_interpreter_sessions       tool_calls      requests
```

不在这张表里的指标，只要有费率行点名它，照样被定价。这张表说的是内置键有哪些，不是一个
过滤器。

`requests` 是每请求费：每个**被定价的交换**计一个请求。

## 分层

一行 `price_tiers` **只**覆盖 token 价格，而且一律按每百万。

| 字段 | 含义 |
| --- | --- |
| `serviceTier` | `null` 使它成为一个**上下文**档位；否则是 `standard`、`priority`、`flex`、`batch`… |
| `minPromptTokens` | 非负的含端点阈值；默认 0 |
| `multiplier` | 服务档位用：把继承来的价格乘以多少。仅上下文的行不设它 |
| `*_per_million` | 某一类 token 的显式价格 |

显式列有 `input`、`output`、`cache_read`、`cache_creation_5m` / `30m` / `1h`、
`reasoning`、`image_input`、`image_output`、`audio_*` 和 `video_*`。

**提示长度算的是 input 加缓存读加缓存写，各计一次。**

### 按 token 类的复合

1. **基础费率**——该指标的 `price_rates` 行，带条件的先试。
2. **上下文档位**——在**没有**服务档位的行里，取已达到的最大 `minPromptTokens`。它的显式
   价格替换掉它所设的那些类的基础价。
3. **服务档位**——在点名了*实际*服务档位的行里，取已达到的最高阈值。那里的显式价格
   **直接胜出**；否则把上下文调整后的基础价乘以该行的 `multiplier`（默认 1）。

阈值相同时按 `(priority, id)` 较小者破平。`null` 表示继承；0 表示**明确免费**，不是缺失。

乘数从不作用于工具或媒体的*计数*——只作用于 token 价格。

:::caution[显式档位价会替换掉整条阶梯]
基础 input 价为 1、上下文台阶 `≥ 200 000 → 2`、提示 300 000 个 token 时：

| `batch` 行 | 实际 input 费率 |
| --- | --- |
| `{"serviceTier":"batch","multiplier":"0.5"}` | `2 × 0.5 = 1` |
| `{"serviceTier":"batch","inputPerMillion":"0.5"}` | `0.5`——200k 那级丢了 |
| `{"serviceTier":"batch","minPromptTokens":200000,"inputPerMillion":"1"}` | `1` |

要么在它必须覆盖的每一级都重复写一遍显式档位价，要么用乘数。
:::

### 两个特例

- **reasoning 是 output 的子集。** 存在 reasoning 价格时，reasoning token 从
  `output_tokens` 里扣出来单独计；不存在时它们留在 output 里，只被计一次。
- **没有价格的缓存读继承 input 价。** 没有价格的*其他*每一类都免费。这个不对称是刻意的：
  缓存读毫无疑义就是一个 input token，而缺失的音频或视频价格是运维者应该看到为 0 的配置缺口。

### 要求的档位 vs 实际提供的

被收费的是响应**实际报告**的那个档位，不是请求要求的那个。一个被厂商降级的 `priority`
请求，按降级后那一行结算。

## 其余一切

不是 token 类的每个指标都按

```text
amount × value / unitQuantity

```

计费，而且**从不被服务档位乘**。

## 编辑价格

```sh
curl -s http://127.0.0.1:8787/admin/api/price-rules  -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:8787/admin/api/price-rates  -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:8787/admin/api/price-tiers  -H "Authorization: Bearer $GPROXY_KEY"
```

每个家族都有通常的五条路由外加一个批量，而一次批量无论点名多少行都是一个 revision 提交。

内置目录能把本次发布知道的模型填进去：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/default-model-catalog/apply-prices \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","modelIds":["gpt-4o-mini"],"overwrite":false}'
```

不带 provider 时使用目录自己的 glob 与优先级，因此一条规则给这个模型在任何地方定价；
带 provider 时字面名字成为 pattern，优先级为 0。**`overwrite: false` 正是让重复应用安全的
东西**：运维者改过的规则保留它的改动，并被报告为跳过。

一行坏的计价数据在装配时被丢掉并打 warn，**不会**阻塞快照。一个打错的小数不该拖垮一个
本来还在正常服务流量的实例。

## 预算花的是这个结果

一个预算就是一行 metric 为 `cost`、unit 为 USD 的 `quotas`。准入把调用方的链
`[api_key?, user, team?, org?]` 交给引擎，而链上**任何** owner 的**每一个**
启用预算都适用。

费用在上游调用结束后结算，在途请求和并发调用可能使预算超出上限。预算归属与检查方式见[权限、限流与费用预算](/zh-cn/guides/permissions/)。

## Token 估算

当一个可计费的响应完全没带用量时，引擎自己数 token，并把记录标为 `estimated = true`。
计数用的是本地词表：GPT 系列用 tiktoken 编码，其他用该模型目录行点名的词表，再没有就用
内置的 DeepSeek。

```sh
curl -s http://127.0.0.1:8787/admin/api/tokenizer-vocabs -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/tokenizer-vocabs \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"repo":"…","filename":"tokenizer.json","modelId":"…","setAsDefault":true}'
```

一次抓取经配置好的文件存储下载文件，然后插入那一行并把模型与默认值指过去——**一行加两个
指针在一个 revision 提交里**。字节先落地、行后落库：一行指向从未被写入的对象会让此后每次
重载都失败，而一个没有行的对象只是浪费空间。

没有文件存储时整个家族回答"不支持"：没有地方放这些字节。
