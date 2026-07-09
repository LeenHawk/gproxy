---
title: 消息改写
description: 无需修改客户端应用，就可以添加系统指令或替换提示词文本。
---

消息改写规则让网关统一调整提示词，不需要每个客户端应用重复修改。根据需求选择合适的规则：

- `system_text` 把文本添加到上游格式的系统指令位置；
- `transform` 在序列化后的 body 或匹配路径上做文本替换；
- `rewrite` 在你知道上游 API 结构时修改具体 JSON 路径。

这些规则在协议 transform 之后运行。这一点很重要：OpenAI 客户端请求如果路由到 Claude，上游 body 会先转成 Claude Messages，然后 message rule 看到的是 Claude body shape。

## `system_text`

用 `system_text` 注入服务端管理的指令：

```json
{
  "text": "Follow the internal safety policy for this workspace.",
  "position": "prepend"
}
```

支持 `prepend` 和 `append`。Runtime 会根据目标 content-generation kind 映射到原生位置：

| Target kind | Native location |
| --- | --- |
| `claude_messages` | `system` 字符串或 `system[]` text block。 |
| `open_ai_chat_completions` | `messages[]` 中 `role: "system"` 的 item。 |
| `open_ai_responses` | `instructions`。 |
| `gemini_generate_content` | `systemInstruction.parts[]`。 |

## `transform`

当结构路径不是合适模型时，用 `transform` 做 regex replacement：

```json
{
  "phase": "request",
  "locate": { "match": "\\bAcme internal\\b" },
  "actions": [{ "op": "replace_text", "with": "the workspace" }]
}
```

Replacement 在序列化后的 provider-native request body 上运行。它可以修改 body 字符串表示中的任意文本。这个能力对 prompt text 有用，但也可能影响你没打算修改的 JSON string value。建议使用 word boundary 和窄 pattern。

## `rewrite`

当你知道 provider-native path 时，用 `rewrite`：

```json
{
  "path": "messages.0.content",
  "action": "set",
  "value_json": "Pinned instruction text"
}
```

它是精确的结构化修改，但不跨协议可移植。Claude system path、OpenAI Chat system message、OpenAI Responses `instructions`、Gemini `systemInstruction` 是不同结构。

## 按 Operation 限定范围

把消息规则限制在内容生成 Operation，避免它们作用于模型列表、Embedding 或图片请求：

```json
["generate_content", "stream_generate_content"]
```

如果规则只适用于某个模型或模型系列，再加上模型过滤条件。

## 与缓存的关系

Claude 和 OpenAI 提示缓存都会精确匹配前缀。如果改写规则修改了 `cache_control` 或
`prompt_cache_breakpoint` 之前的文本，原本预期的缓存命中就可能变成 miss。应先完成稳定内容的
改写，再把缓存断点放在改写后的前缀末尾。
