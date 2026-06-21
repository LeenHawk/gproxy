---
title: Claude Prompt Caching
description: 使用 v2 provider rule set 配置 Claude cache breakpoint 和 beta header。
---

在 v2 中，Claude prompt caching 表示为对 provider-native Claude Messages body 的请求处理。当前 runtime 支持 `cache_breakpoint` 规则，以及 Claude-compatible channel 的 request shaping。

Cache 规则在协议 transform 之后运行。Gemini 或 OpenAI 客户端请求如果路由到 Claude target，也可以插入 Claude cache marker，因为 body 已经转成 `claude_messages`。

## Cache Breakpoint Rule

`cache_breakpoint` 配置形状：

```json
{
  "target": "system",
  "index": 1,
  "ttl": "5m"
}
```

支持字段：

| 字段 | 含义 |
| --- | --- |
| `target` | `system`、`tools` 或 `last_message`。 |
| `index` | Console 侧的有符号索引：`>0` 表示正数第 N 条，`<0` 表示倒数第 N 条，`0` 无效；省略时 runtime 使用最后一个 block。 |
| `ttl` | 可选 TTL 字符串，例如 `5m` 或 `1h`。 |
| `position` | 兼容保留字段；当前未使用。 |

该规则只在目标 operation kind 是 `claude_messages` 时应用。非 Claude target 会 warn 并跳过。

## Targets

| Target | marker 插入位置 |
| --- | --- |
| `system` | Claude `system` array 中的某项。字符串形式的 `system` 不能携带 block metadata，会跳过。 |
| `tools` | 顶层 `tools` array 中的某项。 |
| `last_message` | 最后一条 message 的 `content` array 中的某个 block。 |

Runtime 写入：

```json
{ "cache_control": { "type": "ephemeral" } }
```

如果设置了 `ttl`，会把它加入该 object。

## Beta Headers

Anthropic 的一小时 cache TTL 需要 beta header。在 v2 中，用绑定到同一 provider 的 `header` 规则添加：

```json
{
  "name": "anthropic-beta",
  "value": "extended-cache-ttl-2025-04-11",
  "mode": "merge"
}
```

使用 `merge` 可以保留客户端已有 beta token。

## Rule 顺序

Process 层执行顺序：

```text
system_text -> cache_breakpoint -> rewrite -> sanitize -> header
```

这意味着服务端 system text 会先插入，然后 cache breakpoint 才放置。后续 rewrite 或 sanitize 仍然可能改变 cached content，因此组合 prompt rewrite 和 prompt caching 时要小心。

## 设计方向

当前 `cache_breakpoint` 是专用 kind。计划中的通用 transform model 会把它表达成 structural locator 加 merge action，并由 console preset 生成 provider-specific path。后端应保持宽松：按配置执行 mutation，让上游负责执行 provider policy，例如 breakpoint 数量限制。
