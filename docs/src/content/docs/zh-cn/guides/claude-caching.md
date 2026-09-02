---
title: 提示缓存
description: "用 cache_breakpoint 规则或 GPROXY 魔法字符串放置 Claude 与 OpenAI 缓存断点，并了解缓存读写如何计费"
---

提示缓存按精确前缀匹配，因此长而稳定的指令放在每轮都变化的文本之前时收益最
大。GPROXY 在请求转换为上游线上格式之后再标记该前缀，所以无论客户端说的是
OpenAI、Claude 还是 Gemini，同一套配置都适用。

添加断点有两种方式：

- 在附加到 Provider 的规则集里加一条 `cache_breakpoint` 规则（见
  [路由规则与规则集](/zh-cn/guides/rules/)）。缓存策略由操作员掌握。
- 客户端在提示文本中嵌入**魔法字符串**，适合自己无法发送 `cache_control` 的
  客户端。Provider 必须打开对应的魔法缓存开关。

客户端已经发送的标记在转换中会保留：文本块上的 Claude `cache_control` 变成
OpenAI 的显式 `prompt_cache_breakpoint`，OpenAI 断点变成不带 TTL 的 ephemeral
`cache_control`。

## 缓存断点规则

```json
{ "kind": "cache_breakpoint", "target": "system", "index": null, "ttl": "1h" }
```

| 字段 | 含义 |
| --- | --- |
| `target` | `top_level`（别名 `global`）、`system`、`tools`、`message`。 |
| `index` | 在目标的可缓存块扁平序列中的有符号、从 1 开始的位置。正数从前往后数，负数从后往前数，`null` 选最后一个块，`0` 无效。 |
| `ttl` | Claude：`5m` 或 `1h`，写入 `cache_control.ttl`；省略表示使用 Provider 默认值。OpenAI：`30m` 设置 `prompt_cache_options.ttl`；其他值忽略。 |

按目标格式的行为：

| 目标 | Claude Messages | OpenAI Chat / Responses | Gemini |
| --- | --- | --- | --- |
| `top_level` | 添加请求级 `cache_control`，已存在则跳过。 | 客户端未选择模式时把 `prompt_cache_options.mode` 设为 `implicit`；按需添加 `ttl: "30m"`。 | 跳过 |
| `system` | 标记 `system[]` 中选中的块。 | Chat：一条 `system` 或 `developer` 消息。Responses：`instructions`（GPROXY 插入一个只含一个空格、携带标记的 `developer` 项，因为 `instructions` 本身无法携带）或一个 `system`/`developer` 输入项。 | 跳过 |
| `tools` | 标记一个工具定义。 | 跳过 | 跳过 |
| `message` | 按提示顺序摊平每个 `messages[].content` 的可缓存块，再应用 `index`。 | Chat：除 `function` 角色之外的每条消息。Responses：`input` 字符串，或每个带角色和内容的输入项。 | 跳过 |

字符串内容会在标记前规范化为文本块（Claude）或文本部件（OpenAI），因此纯字符
串的系统提示也能携带断点。

Claude 最多携带四个标记：会成为第五个的规则被跳过，已有 `cache_control` 的块
保持原样。Claude 无法缓存的块永远不会被选中：`thinking`、
`redacted_thinking`、引用与位置块、空文本，以及 `user` 消息之外的图片或文档。
OpenAI 可标记的部件为 `text`、`image_url`、`input_audio`、`file`、`refusal`
（Chat）以及 `input_text`、`input_image`、`input_file`、`output_text`、
`refusal`（Responses）。

生成的标记：

```json
{ "cache_control": { "type": "ephemeral", "ttl": "1h" } }
```

```json
{ "type": "input_text", "text": "…", "prompt_cache_breakpoint": { "mode": "explicit" } }
```

## 魔法字符串

无法设置 `cache_control` 的客户端可以在文本块中放入三个固定字符串之一。GPROXY
在请求发出前删除该字符串，并在该块上放置原生标记。

| 字符串 | Claude 结果 | OpenAI 结果 |
| --- | --- | --- |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH` | 使用 Provider 默认 TTL 的 `cache_control` | 显式断点 |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF` | `ttl: "5m"` 的 `cache_control` | 显式断点 |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY` | `ttl: "1h"` 的 `cache_control` | 显式断点 |

这些字符串是冻结的：它们属于客户端到代理的协议，不会随版本变化。行为如下：

- 该处理按 Provider 通过两个类型化设置 `enable_claude_magic_cache` 和
  `enable_openai_magic_cache` 启用，位于 Provider 的高级设置中。Claude 开关存
  在于 Claude API、Claude Code、AWS Bedrock、Azure、Custom、OpenCode、
  OpenRouter 和 Vercel 通道；OpenAI 开关存在于 OpenAI、Codex、Azure、Custom、
  OpenCode、OpenRouter 和 Vercel。
- 三个字符串的每一次出现都会从每个 `text` 字段中删除，即使已达标记上限。
- 已有标记计入每请求四个的上限。达到四个后，后续字符串仍被删除，但不再写入
  标记。
- 在 Claude 目标上，该处理在规则集之后运行，因此一条 `system_text` 规则本身
  也可以携带魔法字符串。GPROXY 会先规范化请求体：字符串内容变成文本块，空文
  本块被丢弃，落在被丢弃块上的标记移到前一个可缓存块。
- 在 OpenAI 目标上，该处理在通道内运行。Chat 消息字符串变成带标记的 `text`
  部件；被标记的 Responses `instructions` 字符串被清理，并由一个只含一个空格
  的 `developer` 项携带断点；`prompt.variables` 以及 `input` 字符串或部件就地
  标记。三个字符串产生同一种显式断点，因为 OpenAI 没有块级 TTL。

## Claude Provider 上的 OpenAI 客户端

来自 OpenAI 格式客户端、被路由到 Claude Provider 的请求会在任何规则运行之前
转换为 Claude Messages。因此该 Provider 上的 `cache_breakpoint` 规则写入
`cache_control`，客户端文本中的魔法字符串在 Claude 开关下同样生效。客户端继
续收到自己格式的响应。

## 预设

规则工作区内置两个缓存预设，都过滤 `generate_content` 与
`stream_generate_content`，TTL 为 `1h`：**Claude system cache** 标记最后一个
`system` 块，**Claude message cache** 标记最后一个可缓存消息块。应用预设会创
建一个附加到该 Provider 的普通、可编辑规则集。没有任何通道会自动播种缓存规则。

## 用量与计费

Claude 在 `usage` 中报告缓存活动；GPROXY 的映射如下。

| Claude usage 字段 | GPROXY | 控制台列 |
| --- | --- | --- |
| `cache_read_input_tokens` | `cached_input_tokens`；同时计入 `input_tokens` | 缓存读取 |
| `cache_creation.ephemeral_5m_input_tokens` | 指标 `cache_creation_5m_tokens` | 缓存写入 5min |
| `cache_creation.ephemeral_1h_input_tokens` | 指标 `cache_creation_1h_tokens` | 缓存写入 1h |
| 旧版 `cache_creation_input_tokens` | 指标 `cache_creation_5m_tokens` | 缓存写入 5min |

结算按规则的 `cache_read_price` 为缓存 Token 计价，未设置时回落到输入价格。缓
存写入按层级行上的 `cache_creation_5m_price`、`cache_creation_30m_price` 和
`cache_creation_1h_price` 以每百万 Token 计价，或按匹配该指标的维度
`price_rates` 行计价；两者都没有时，缓存写入按零结算。当上下文阶梯选择层级
时，缓存创建 Token 计入 `min_prompt_tokens` 阈值。见
[价格与分层](/zh-cn/reference/pricing/)。

## 规则顺序与缓存命中

请求规则按 `system_text -> cache_breakpoint -> rewrite -> transform ->
header` 运行，因此靠后的 `rewrite` 或 `transform` 仍可能改动断点之前的文本，
把预期的命中变成未命中。稳定内容放在最前，断点放在其后，每次请求都变化的文本
放在边界之后。
