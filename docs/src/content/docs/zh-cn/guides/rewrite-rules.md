---
title: 请求改写规则
description: 在请求到达上游之前，按模型 / 操作 / 协议过滤，改写或删除请求体中任意的 JSON 字段。
---

GPROXY 允许你在请求发给上游之前，改写或删除**请求体**中的字段。当客户端
死活不肯改掉 `temperature: 1.0` 而模型又不接受、或者你需要强制
`stream_options.include_usage = true`、或者某个供应商需要注入特殊字段时，
这就是你的"逃生出口"。

:::note
本页讲的是 `rewrite_rules` —— 操作**任意 JSON 字段**的改写。如果你要对
系统提示和消息里的**文本内容**做正则替换，请看
[消息改写规则](/zh-cn/guides/message-rewrite/)。两个功能互相独立，可以在
同一个供应商上共存。
:::

改写规则按**供应商**定义，保存在供应商的 settings JSON 里。它在 handler
层被应用，**早于**别名解析，使用的是客户端发送的原始模型名 —— 所以按
别名名匹配的 filter 仍然有效。

实现：[`sdk/gproxy-provider/src/utils/rewrite.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/utils/rewrite.rs)。

## 规则结构

```jsonc
{
  "path":   "temperature",               // 点号路径
  "action": { "type": "set", "value": 0.7 },
  "filter": {                            // 可选 —— AND 逻辑
    "model_pattern": "gpt-4*",
    "operations":    ["generate_content", "stream_generate_content"],
    "protocols":     ["openai_chat_completions"]
  }
}
```

- **`path`** —— JSON body 的点号路径。`temperature` / `stream_options.include_usage` /
  `metadata.tenant` 之类。`set` 操作会自动创建缺失的中间对象。
- **`action.type`** —— 要么 `set` (附带 `value`)，要么 `remove`。
- **`filter`** (可选) —— 限制规则只对哪些请求生效。所有维度是 AND 逻辑，
  缺省的维度匹配一切。

规则列表按顺序依次应用 —— 后一条可以覆盖前一条在同一路径上的效果。

## Action 语义

### `set`

按 path 走下去，途中不存在的对象会被创建，最后在叶子上写入 `value`。
`value` 可以是任意 JSON 值（标量、对象、数组）。如果中间的某个 key 存在
但不是对象（比如客户端发来了 `"a": "string"` 而你去改写 `a.b.c`），
GPROXY 会用一个新对象覆盖它。

```jsonc
// 对所有 OpenAI 流式请求强制打开 include_usage。
{
  "path":   "stream_options.include_usage",
  "action": { "type": "set", "value": true },
  "filter": { "protocols": ["openai_chat_completions"] }
}
```

### `remove`

走到父节点并删除叶子 key。路径不存在则静默跳过。

```jsonc
// 砍掉永远发 1.0 的客户端的 temperature。
{
  "path":   "temperature",
  "action": { "type": "remove" }
}
```

## Filter 语义

| 维度 | 含义 | 匹配方式 |
| --- | --- | --- |
| `model_pattern` | 针对客户端发来的模型名做 glob 匹配。 | `*` (任意字符序列)、`?` (恰好一个字符)。详细行为见 [glob 测试](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/utils/rewrite.rs)。 |
| `operations` | 允许的 `OperationFamily` 列表。 | 当前请求操作必须在列表里，规则才会触发。 |
| `protocols` | 允许的 `ProtocolKind` 列表。 | 当前请求协议必须在列表里，规则才会触发。 |

`model_pattern` 是对**客户端发送的原始模型名**匹配的 —— *早于*别名解析，
也*早于*去除 provider 前缀。因此写 `chat-default` 的过滤器能匹配到
"客户端发来别名 `chat-default`"这种情况，而不是匹配到底层真实模型。

## 在管道中的位置

规范的顺序是：

```text
permission  →  rewrite  →  alias  →  execute
```

Rewrite 在权限检查**之后**（被拒的请求不会触发任何修改），别名解析
**之前**（filter 看到的是客户端输入的那个别名名）。

## 示例

### 1. 对某一族模型强制采样上限

```jsonc
{
  "path":   "temperature",
  "action": { "type": "set", "value": 0.7 },
  "filter": { "model_pattern": "o3*" }
}
```

### 2. 给每次调用注入必需的 `metadata.tenant`

```jsonc
{
  "path":   "metadata.tenant",
  "action": { "type": "set", "value": "acme-prod" }
}
```

中间对象会被自动创建 —— 你不需要先单独 set 一个 `metadata`。

### 3. 只对 Claude 客户端删掉一个字段

```jsonc
{
  "path":   "thinking",
  "action": { "type": "remove" },
  "filter": { "protocols": ["claude"] }
}
```

### 4. 只对 OpenAI 流式请求改写 `stream_options`

```jsonc
{
  "path":   "stream_options",
  "action": {
    "type":  "set",
    "value": { "include_usage": true }
  },
  "filter": {
    "operations": ["stream_generate_content"],
    "protocols":  ["openai_chat_completions"]
  }
}
```

## 顺序与 YAGNI

- 规则**按顺序**应用。两条规则触到同一个 path 时，后一条胜出。
- 非 object 的请求体（空 GET、数组、null）直接跳过 —— 这套规则只对
  JSON 对象类的 body 有意义。
- **不**支持正则；`model_pattern` 是一个简单的 glob。
- 不支持"读出现有值再计算新值" —— 这是一套 set/remove 机制，不是脚本
  引擎。如果你需要基于*值*做条件判断，考虑在客户端处理，或者提议做成
  一个专门的通道扩展。

## 在哪里配置

两个地方：

- **种子 TOML** —— 供应商的 `settings.rewrite_rules` 是一个 JSON 数组。
  通常最方便的做法是先在控制台里组装，然后导出 TOML，而不是手写 JSON。
- **内嵌控制台** —— *供应商 → {你的供应商} → Settings* 提供了规则列表的
  结构化编辑器。改动在下一次请求时生效，不需要 reload。

目前在 settings 里暴露 `rewrite_rules` 的通道包括 OpenAI 兼容、Anthropic、
Claude Code 以及其它大多数通道。
