---
title: 消息改写规则
description: 对请求中的文本字段（系统提示、用户消息、Gemini parts、OpenAI instructions）做正则替换。
---

**消息改写规则**（内部叫 `sanitize_rules`）允许你用**正则 pattern →
replacement** 这种形式改写请求中的**文本内容** —— 系统提示、用户/助手
消息、Gemini 的 `parts[*].text`、OpenAI Responses 的 `instructions` /
`input` 等等。它是协议感知的：GPROXY 会根据上游将要看到的 wire 方言，
走到对应字段里去改。

这和 [请求改写规则](/zh-cn/guides/rewrite-rules/) 是**两个不同的功能**。
请求改写规则作用在请求体的任意 JSON 字段上（temperature、stream_options、
metadata 等），消息改写规则只动文本。两者可以在同一个供应商上共存 ——
它们碰的根本不是同一块东西。

| | 请求改写规则 | 消息改写规则 |
| --- | --- | --- |
| 配置字段 | `rewrite_rules` | `sanitize_rules` |
| 规则形状 | `{ path, action: set \| remove, filter }` | `{ pattern, replacement }` |
| 作用对象 | 请求体的任意 JSON 路径 | 只针对系统提示和消息内容中的文本 |
| 匹配语言 | 点号路径 + glob filter | 对字符串内容做**正则**匹配 |
| 典型场景 | 强制/删除字段、注入 metadata | 改写品牌名、擦掉工具标识、替换短语 |

实现：[`sdk/gproxy-channel/src/utils/sanitize.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-channel/src/utils/sanitize.rs)。

## 规则结构

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\bPi documentation\\b", "replacement": "Harness documentation" },
    { "pattern": "\\bpi\\b",               "replacement": "the agent" },
    { "pattern": "\\bPi\\b",               "replacement": "The agent" }
  ]
}
```

- **`pattern`** —— 一个 [Rust `regex` crate](https://docs.rs/regex/latest/regex/)
  支持的正则。强烈建议加词边界 `\b`，否则短 pattern 会误伤 `pipeline`、
  `api`、`spirit` 这类包含目标字符的词。
- **`replacement`** —— 字面替换文本。它会直接传给 `Regex::replace_all`，
  所以如果你用了捕获组，`$1`、`$2` 反向引用是可以用的。
- 规则按**声明顺序**依次应用。请**把较长的短语放在前面**，单字的短
  pattern 放在后面 —— 否则短 pattern 会先把长短语的前缀吃掉，长规则
  就永远没机会触发了。
- 非法正则 pattern 在编译阶段被静默丢弃（通道会打日志，但不会让请求
  失败）。

## 会改写哪些字段

规则集每次请求编译一次，然后由一个协议感知的 walker 去走对应字段。具体
改哪些字段取决于**上游协议**（不是客户端协议）。

| 上游协议 | 被改写的字段 |
| --- | --- |
| `claude` | `system`（字符串或 `[{type:"text", text}]` 数组）、`messages[*].content`（字符串或文本块数组） |
| `openai_chat_completions` / `openai` | `messages[*].content` |
| `openai_response` | `instructions`、`input`（字符串或 item 数组 —— 每个 item 的 `content` / `output`） |
| `gemini` / `gemini_ndjson` | `systemInstruction.parts[*].text`、`contents[*].parts[*].text`、`generationConfig.contents[*].parts[*].text` |

不管哪种协议，**只有这些字段里的文本**会被动到。二进制片段、图片 URL、
工具调用、结构化 metadata 都不会被碰 —— walker 是显式地往文本块里钻，
不会动别的东西。

## 在管道中的位置

两套改写都在通道的 `finalize_request(...)` 里运行 —— 协议翻译**之后**、
凭证选择和 HTTP 传输封装**之前**。这就是为什么消息改写看到的是**上游**
协议的字段布局：轮到它运行时，请求体已经被翻译成上游期望的形状了。

完整的单次请求顺序是：

```text
permission  →  请求改写  →  alias  →  transform  →  消息改写  →  缓存断点  →  上游
```

## 示例

### 1. 把上游烙印进提示里的 agent 名改掉

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\bPi documentation\\b", "replacement": "Harness documentation" },
    { "pattern": "\\bpi\\b",               "replacement": "the agent" },
    { "pattern": "\\bPi\\b",               "replacement": "The agent" }
  ]
}
```

"长的在前"的顺序非常重要：如果两字符的 `\bpi\b` 先跑，就会把
`Pi documentation` 里的 `Pi` 先吃掉，留下一个支离破碎的
`the agent documentation`，长规则就来不及生效了。

### 2. 擦掉工具标识的前缀

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\bclaude-code://([a-z0-9_-]+)", "replacement": "internal://$1" }
  ]
}
```

用捕获组把工具 id 保留下来，只改写 URL scheme。适合某些上游 assistant
把厂商特有的 scheme 写进了系统提示、又不想让客户端看到的场景。

### 3. 删掉一个尾巴上的签名行

```jsonc
{
  "sanitize_rules": [
    { "pattern": "\\n+—\\s*Sent from my Claude app", "replacement": "" }
  ]
}
```

空 replacement 等于删除。对于那些会泄漏到用量统计/日志里的噪声签名很有用。

### 4. 整理 Gemini 输入的空白

```jsonc
{
  "sanitize_rules": [
    { "pattern": "[\\t ]{2,}", "replacement": " " }
  ]
}
```

Gemini 对某些 `parts[*].text` 块的前导空白比较挑剔 —— 在这儿把连续空格/
tab 压缩成一个，比去改客户端便宜。

## 注意事项

- **会影响 Claude 缓存 key。** Anthropic 按前缀的*精确*文本算缓存 key。
  如果 sanitize 规则动到了被缓存前缀里的文本，每次请求都会变成 cache miss。
  要么把 sanitize 规则的作用范围限制在前缀之外，要么把
  [缓存断点](/zh-cn/guides/claude-caching/)放到被改写内容之后。
- **词边界救你一命。** `\bpi\b` 几乎永远是你想要的；裸 `pi` 会啃掉
  `pipeline`、`api`、`spirit`、`typing` 等等。边界情况可以看现成的单测
  ([`utils/sanitize.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-channel/src/utils/sanitize.rs))。
- **只动文本字段，不动工具 payload。** 工具调用块、JSON 参数、图片 URL
  永远不会被碰到 —— 即使正则在文本化后理论上能匹配到它们。
- **按供应商生效，不按用户。** 没有按用户或模型过滤的 filter —— sanitize
  规则会作用于经过该供应商的每一次请求。如果你需要按用户做文本擦除，
  请放在 GPROXY 上游的某一层做。

## 在哪里配置

- **种子 TOML** —— 每个供应商的 `settings.sanitize_rules` 是一个 JSON
  数组，位置和结构和上面的例子一样。
- **内嵌控制台** —— *供应商 → {你的供应商} → Settings* 提供了规则列表
  的结构化编辑器。改动在下一次请求时生效，不需要 reload。

目前在 settings 里暴露 `sanitize_rules` 的通道包括 `anthropic`、
`claudecode`、OpenAI 兼容通道以及其它大多数通道（看该通道 settings
结构体上的 `ChannelSettings::sanitize_rules` 方法即可确认）。
