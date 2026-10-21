---
title: 提示缓存
description: "通过缓存断点规则或魔法字符串配置提示缓存，查看用量与计价。"
---

提示缓存由上游提供，命中情况取决于模型、提示前缀和有效期。GPROXY 可以通过缓存断点规则或提示中的魔法字符串添加标记；添加标记不保证命中，也不能让不支持缓存的上游获得缓存能力。

## 在控制台添加缓存断点

在供应商的重写规则中新增“缓存断点”，选择上游协议和位置。位置包括全局、系统和消息；Claude 还支持工具。局部位置可填写索引，正数从 1 开始，负数从末尾计数，留空选择最后一个可用位置。

Claude 可选择默认、5 分钟或 1 小时 TTL；OpenAI 局部断点可选择默认、30 分钟或 12 小时；全局规则可选择默认或 30 分钟。实际生效方式取决于上游。规则中的协议是转换后的上游协议，Gemini 不提供这个规则类型。

通过 API 配置时使用 `action: "cache_breakpoint"`，`replacement` 是配置对象的 JSON 字符串，例如：

```json
{"dialect":"claude","target":"message","index":-1,"ttl":"5m"}
```

```json
{"dialect":"openai","target":"message","index":-1,"ttl":"12h"}
```

这条规则作用于请求正文，与其他重写规则按配置顺序执行。客户端能自行提交缓存标记时，也可以直接使用上游支持的字段。

## 使用魔法字符串

无法直接设置缓存字段的客户端，可以在提示文本中放置以下标记，并启用供应商对应的 magic cache 选项。

### 三个字符串

```text
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY
```

| 字符串 | 在 Claude 目标上 | 在 OpenAI 目标上 |
| --- | --- | --- |
| …`7D9ASD…` | `cache_control`，用 API 的默认 TTL | 一个显式断点 |
| …`49VA1S…` | `cache_control`，`"ttl": "5m"` | `prompt_cache_breakpoint` (`"ttl": "30m"`) |
| …`1FAS5G…` | `cache_control`，`"ttl": "1h"` | `prompt_cache_breakpoint` (`"ttl": "12h"`) |

同一组字符串按目标协议选择短期和长期缓存：Claude 的 `5m` / `1h` 对应 OpenAI 的 `30m` / `12h`，默认字符串不指定 TTL。局部规则将 TTL 写在选中的断点上，不修改整个请求的缓存选项。

原生缓存字段在 Claude ↔ OpenAI Chat/Responses 转换中也使用同一双向映射。有标记的系统提示、消息、图片/文档和工具结果保留块边界。Claude 的自动缓存策略在末尾块有目标协议表示时转换为该块的显式断点；没有 OpenAI 对应字段的工具定义级标记仍由转换报告列出。

OpenAI 转 Claude 时，只有明确传入 `prompt_cache_options.mode: "implicit"` 才生成顶层 `cache_control`。省略整个选项对象或省略 `mode`（包括空对象、只传 TTL）都不会启用 Claude 自动缓存；块上的显式标记仍独立转换。

它们是**冻结的**：它们是客户端到代理之间协议的一部分，不随版本变化。

## 最要紧的那条规则

**无论缓存是否启用，token 总是被剥掉。** 一个泄漏到别人提示里的标记，任何时候都不是
想要的东西。

开关决定的只是要不要在 token 所在的位置放一个*断点*。magic cache 关闭时，token 被取走，
别的什么也不动：客户端自己的 `cache_control` 或 `prompt_cache_breakpoint` 原样保留，
而不含 token 的 body 逐字节转发，既不解析也不重新序列化。

只剥不放的那一趟也正是 `claudeweb` 所做的，因为浏览器会话根本没有缓存控制。

## 启用它

两个供应商 `config` 开关，每族一个：

```json
{ "config": { "enable_claude_magic_cache": true,
              "enable_openai_magic_cache": true } }
```

| 开关 | 提供它的渠道 |
| --- | --- |
| `enable_claude_magic_cache` | `aws_bedrock`、`azure`、`claudeapi`、`claudecode`、`custom`、`opencodego`、`opencodezen`、`openrouter` |
| `enable_openai_magic_cache` | `aws_bedrock`、`azure`、`openai`、`codex`、`custom`、`opencodego`、`opencodezen`、`openrouter` |

开关是对着**目标方言**读的，不是客户端的。一个被路由到 Claude 供应商的 OpenAI 形状
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
| 缓存写入，按 TTL 分桶 | `cache_creation_tokens` + `ttl_seconds` |
| 缓存写入，TTL 未知 | `cache_creation_tokens` |

缓存写入统一使用 `cache_creation_tokens`。费率条件 `{"ttl_seconds":43200}` 匹配 12 小时写入，省略该条件则为默认价格；同一次请求中的各个写入桶分别计费。缓存读取仍是单一的 `cached_input_tokens` 计数，没有 TTL。阶梯价格使用 `cacheCreationPerMillion: [{"ttlSeconds":43200,"price":"5"}]`，其中 `ttlSeconds: null` 表示默认价格。价格按百万 Token 计，不能再乘 TTL。

OpenAI Responses 和 Chat 的显式断点支持 `prompt_cache_breakpoint: {"mode":"explicit","ttl":"12h"}`（也支持 `"30m"`），Chat ↔ Responses 转换会保留这个 TTL。当前[官方 OpenAPI](https://github.com/openai/openai-openapi/blob/master/openapi.json) 的请求级 `prompt_cache_options.ttl` 仍只列出 `"30m"`；`prompt_cache_retention` 是独立的旧策略。单次 usage 目前只声明 `cache_write_tokens` 总量，因此 GPROXY 将其 TTL 记为未知，不猜测混合断点的写入分布。

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
