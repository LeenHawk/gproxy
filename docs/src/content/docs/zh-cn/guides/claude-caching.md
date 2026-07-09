---
title: 提示缓存
description: 通过 Provider 规则或 GPROXY 魔法字符串添加 Claude 和 OpenAI 缓存断点。
---

提示缓存适合这样的请求：前面是长而稳定的系统指令、工具说明或参考资料，后面才是每次都会
变化的用户内容。GPROXY 会先把请求转换成上游协议，再插入对应协议的缓存断点。因此客户端和
上游即使使用不同 API 格式，也可以使用同一类规则。

有两种添加断点的方式：

- 缓存策略由 Provider 管理时，使用 `cache_breakpoint` 规则；
- 需要客户端在提示词里决定边界时，打开 **魔法字符串缓存**。

## 手动断点规则

新建一条 `kind: "cache_breakpoint"` 的规则。例如，下面的配置会标记 OpenAI 请求中的系统
前缀，并使用 OpenAI 请求级的 30 分钟 TTL：

```json
{
  "target": "system",
  "ttl": "30m"
}
```

支持以下字段：

| 字段 | 含义 |
| --- | --- |
| `target` | `top_level`、`system`、`tools` 或 `last_message`。不同协议支持的目标不完全相同，见下表。 |
| `index` | 从 1 开始的有符号块索引。正数从前往后数，负数从后往前数；省略时选择最后一块。`0` 无效。 |
| `ttl` | Claude 使用 `5m` 或 `1h`；OpenAI 使用 `30m`。 |
| `position` | 为兼容旧配置保留，目前不生效。 |

规则在协议转换之后执行。OpenAI 客户端如果被路由到 Claude 上游，规则看到的是 Claude
Messages 请求，并写入 Claude 断点；目标是 OpenAI Chat 或 Responses 时，则写入 OpenAI
断点。

### 各目标的行为

| Target | Claude Messages | OpenAI Chat / Responses |
| --- | --- | --- |
| `top_level` | 在请求顶层添加 `cache_control`，启用 Anthropic 自动提示缓存。 | 在请求未指定模式时，确保 `prompt_cache_options.mode` 为 `implicit`。 |
| `system` | 标记 `system` 数组中的内容块。字符串形式的 `system` 无法携带块元数据，会跳过。 | 标记 system/developer 内容。Responses 的 `instructions` 本身不能携带断点，GPROXY 会紧接着插入一个很小的 developer 内容块作为缓存边界。 |
| `tools` | 标记工具定义。 | OpenAI 不支持，规则会跳过并记录警告。 |
| `last_message` | 标记最后一条消息中的内容块。 | 标记 Chat 最后一条消息或 Responses 最后一条输入消息中的受支持内容块。 |

## OpenAI 断点

OpenAI Chat Completions、Responses 和 Responses WebSocket 会在受支持的内容块上使用：

```json
{
  "type": "input_text",
  "text": "稳定的参考资料",
  "prompt_cache_breakpoint": {
    "mode": "explicit"
  }
}
```

Chat Completions 对应的文本块类型是 `text`。OpenAI 的 TTL 属于整个请求，因此规则中的
`"ttl": "30m"` 还会写入：

```json
{
  "prompt_cache_options": {
    "ttl": "30m"
  }
}
```

添加断点不会覆盖客户端已经传入的 `prompt_cache_options.mode`。保留默认的 `implicit`，可以
同时使用 OpenAI 自动断点和显式断点；设置为 `explicit`，则只使用你明确标记的边界。

显式断点需要 GPT-5.6 或更新的模型系列。旧模型可能拒绝 `prompt_cache_options` 和
`prompt_cache_breakpoint`。具有相同前缀的请求应使用稳定的 `prompt_cache_key`，以获得更可靠
的匹配。OpenAI 只缓存达到最低 token 数的前缀，因此很短的提示词即使有断点也不会命中缓存。

## Claude 断点

Claude 在选中的内容块上使用 `cache_control`：

```json
{
  "cache_control": {
    "type": "ephemeral",
    "ttl": "5m"
  }
}
```

使用 Claude 一小时 TTL 时，还需要给同一 Provider 绑定一条 `header` 规则：

```json
{
  "name": "anthropic-beta",
  "value": "extended-cache-ttl-2025-04-11",
  "mode": "merge"
}
```

使用 `merge` 可以保留客户端已经请求的其他 beta 功能。

## 魔法字符串

在 Provider 设置中打开 **魔法字符串缓存** 后，客户端可以把触发字符串直接放进文本块。
该开关适用于 OpenAI、Codex、Claude API、Claude Code、OpenRouter 和 Vercel 渠道。GPROXY
会在发送上游前删除触发字符串，并在相同位置添加目标协议的缓存断点。

Claude 和 OpenAI 格式共用以下触发字符串：

| 触发字符串 | Claude 结果 | OpenAI 结果 |
| --- | --- | --- |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH` | 使用 Provider 默认 TTL 的 `cache_control` | 显式断点 |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF` | `5m` 的 `cache_control` | 显式断点 |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY` | `1h` 的 `cache_control` | 显式断点 |

OpenAI 当前只支持请求级的 `30m` TTL，因此三种字符串在 OpenAI 请求中会生成相同的显式
断点。已有断点也会计入最多四个断点的限制。超过限制后，触发字符串仍会被删除，但不会继续
添加断点。

## 执行顺序与缓存命中

请求规则按以下顺序执行：

```text
system_text -> cache_breakpoint -> rewrite -> transform -> header
```

后续的 rewrite 或 transform 仍然可能修改断点之前的文本。只要前缀发生变化，原本预期的缓存
命中就可能变成 miss。建议把稳定内容放在最前面，在其后添加断点，再放每次请求都会变化的
内容。
