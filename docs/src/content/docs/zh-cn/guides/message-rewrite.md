---
title: Message Rewrite
description: 用 v2 的 system_text、sanitize 和 generic rewrite 规则改写消息文本。
---

v2 没有独立的 "message rewrite" 表。面向消息文本的改写通过 provider rule-set 系统表达：

- `system_text` 把文本插入 provider-native 的 system 位置；
- `sanitize` 在序列化后的 body 上做文本 pattern replacement；
- `rewrite` 在你知道 provider-native 结构时修改具体 JSON path。

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

这是当前少数知道协议语义的 rule kind。v2 的设计偏好是：通用 transform engine 存在后，把这种 provider-specific path 选择移到 frontend/config preset 中。

## `sanitize`

当结构路径不是合适模型时，用 `sanitize` 做 regex replacement：

```json
{
  "pattern": "\\bAcme internal\\b",
  "replacement": "the workspace"
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

Message rewrite 通常应过滤到内容生成 Operation：

```json
["generate_content", "stream_generate_content"]
```

不要按 provider-family 组织行为。v2 的分类以 `Operation` 和 `OperationGroup` 为中心；provider 或 protocol kind 是该 Operation 内的 wire shape。

## 与缓存的关系

Claude prompt cache key 依赖 prefix 的精确内容。如果 message rewrite 修改了 `cache_control` breakpoint 之前的文本，每次请求都可能变成 cache miss。把稳定的 cache breakpoint 放在改写内容之后，或者让 rewrite 规则避开 cached prefix。
