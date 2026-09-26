---
title: 提示缓存
description: GPROXY v4 怎么放置提示缓存断点：三个魔法字符串、哪些渠道认它们、各方言把标记放在哪，以及缓存 token 怎么计价。
---

提示缓存匹配的是精确前缀，因此当长而稳定的指令排在每轮都变的文本之前时，缓存才划算。
放置断点是客户端的事——但很多客户端做不到，因为它们的请求形状里没有这个字段。

对这些客户端，GPROXY 读取嵌在提示文本里的**魔法字符串**。这就是 v4 的全部机制：没有
`cache_breakpoint` 规则 kind，也没有 cache 预设。两者在 v3 都存在，都没有被移植。

## 三个字符串

```text
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY
```

| 字符串 | 在 Claude 目标上 | 在 OpenAI 目标上 |
| --- | --- | --- |
| …`7D9ASD…` | `cache_control`，用 API 的默认 TTL | 一个显式断点 |
| …`49VA1S…` | `cache_control`，`"ttl": "5m"` | 一个显式断点 |
| …`1FAS5G…` | `cache_control`，`"ttl": "1h"` | 一个显式断点 |

三个在 OpenAI 上产生同一个断点，因为 OpenAI 没有按块的 TTL。

它们是**冻结的**：它们是客户端到代理之间协议的一部分，不随版本变化。

## 最要紧的那条规则

**无论缓存是否启用，token 总是被剥掉。** 一个泄漏到别人提示里的标记，任何时候都不是
想要的东西。

开关决定的只是要不要在 token 所在的位置放一个*断点*。magic cache 关闭时，token 被取走，
别的什么也不动：客户端自己的 `cache_control` 或 `prompt_cache_breakpoint` 原样保留，
而不含 token 的 body 逐字节转发，既不解析也不重新序列化。

只剥不放的那一趟也正是 `claudeweb` 所做的，因为浏览器会话根本没有缓存控制。

## 启用它

两个 Provider `config` 开关，每族一个：

```json
{ "config": { "enable_claude_magic_cache": true,
              "enable_openai_magic_cache": true } }
```

| 开关 | 提供它的渠道 |
| --- | --- |
| `enable_claude_magic_cache` | `aws_bedrock`、`azure`、`claudeapi`、`claudecode`、`custom`、`opencodego`、`opencodezen`、`openrouter` |
| `enable_openai_magic_cache` | `aws_bedrock`、`azure`、`openai`、`codex`、`custom`、`opencodego`、`opencodezen`、`openrouter` |

开关是对着**目标方言**读的，不是客户端的。一个被路由到 Claude Provider 的 OpenAI 形状
请求会先转成 Claude Messages，因此起作用的是 Claude 开关，写下的标记是 `cache_control`。
客户端收到的仍然是它自己的格式。

## 标记落在哪

### Claude Messages

body 先被规范化：字符串 content 变成块数组，空文本块被丢弃，落在被丢弃块上的
`cache_control` 移到最近的可缓存块上——这正是让一个单独成行的 token 也能工作的原因。
thinking 块只在 assistant 轮次保留。

然后 `system` 在 `messages` **之前**被遍历，因此预算按提示顺序花掉，而不是按 JSON 键序。
每个带 token 的文本块都被标记：

```json
{ "cache_control": { "type": "ephemeral", "ttl": "1h" } }
```

已经带 `cache_control` 的块保持不动。

### OpenAI Chat Completions

除 `function` 角色外的每条消息都可以被标记。纯字符串 `content` 会被拆成一个被标记的
`text` part，因为标记需要一个落脚处；数组 content 则就地标记它的 part。

```json
{ "type": "text", "text": "…", "prompt_cache_breakpoint": { "mode": "explicit" } }
```

### OpenAI Responses

三个位置，依次是：

1. **`instructions`。** 那里的 token 被剥掉，并在 `input` 前面插入一条被标记的 developer
   消息——`instructions` 是字符串，自己承载不了标记。
2. **`prompt.variables`**，像 Chat 的 part 一样标记。
3. **`input`**，无论它是裸字符串（变成一条被标记的 user 消息）还是一组 item，后者的
   `content` 与 `output` 用 `input_text` / `output_text` 类型标记。

### Gemini

没有断点。token 被剥掉，别的什么都不发生。

## 四个的上限

**最多四个断点离开代理，含客户端自己的。** 预算是数 body 里已有的标记算出来的，所以一个
自己放了三个的客户端还剩一个名额。

超出上限的 token 仍然被剥掉——只是不变成标记。这和别处是同一条规则：这些字符串永远不会
到达上游。

## 用量与计价

缓存 token 有自己的指标，结算时按你的费率行定价：

| 上游报了什么 | 指标 |
| --- | --- |
| 缓存读 | `cached_input_tokens` |
| 5 分钟缓存写 | `cache_creation_5m_tokens` |
| 30 分钟缓存写 | `cache_creation_30m_tokens` |
| 1 小时缓存写 | `cache_creation_1h_tokens` |

`cached_input_tokens` 以 `input_tokens` 为上限：未命中的余量按 input 价计，命中部分按
缓存读价计，而缓存读**没有价时继承 input 价**。没有自己费率行的缓存写按 0 结算。见
[价格与分层](/zh-cn/reference/pricing/)。

单条记录上的 token 数是可空的——上游没报某个字段，不等于它测到了 0——而每个合计都是普通
数值，因为"什么都没报"的求和确实是 0。

## 保住命中

改写规则跑在转换**之后**、渠道之前，所以一条改提示文本的规则可以把一次本该命中的缓存变成
未命中。把稳定内容放在最前、标记放在它之后、每请求都变的内容全部放在边界之后——并且检查
没有哪条 `paths` 选得过宽的改写规则在改这段前缀。

见[改写规则与操作覆盖](/zh-cn/guides/rules/)。
