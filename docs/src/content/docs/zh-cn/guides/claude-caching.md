---
title: Claude 提示缓存
description: anthropic / claudecode 通道如何对接 Anthropic 提示缓存 —— 包括缓存断点规则和魔法字符串触发器。
---

Anthropic 的**提示缓存 (prompt caching)** 允许你复用请求中静态前缀部分
（工具列表、系统提示、长上下文）的 KV 缓存 —— 按打折的"缓存读取"
(cache read) token 费率付费，而不是每次都重新处理前缀。详见 Anthropic
官方文档：
[Prompt caching — platform.claude.com](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)。

GPROXY 需要在其之上遵守的 Anthropic 侧规则：

- 一次请求最多可以标注 **4 个缓存断点**，通过在某个内容块上设置
  `cache_control: { type: "ephemeral" }`。
- 断点是**位置性**的：按规范顺序位于断点*之前*的全部内容都会进缓存，
  以内容相等作为命中 key。
- 缓存条目是**临时的 (ephemeral)**，有两种存活时间 —— **5 分钟**（默认）
  和 **1 小时**（需要 `extended-cache-ttl-2025-04-11` beta 请求头）。
- 缓存作用域 = 模型 + beta 请求头集合 + 工具定义 + system 内容 ——
  前缀里任意一处变化都会让后续全部缓存失效。

`anthropic` 和 `claudecode` 这两个通道都额外提供两种**在服务端注入断点**
的方式，让不会设置缓存的客户端也能吃到这一波折扣：

1. **缓存断点规则** (`settings.cache_breakpoints`) —— 服务端按位置性规则
   自动注入。
2. **魔法字符串触发器** (`settings.enable_magic_cache`) —— 客户端在文本
   里埋一个约定好的标记字符串，服务端把它改写成 `cache_control` 块。

两者都在 `finalize_request(...)` 阶段对请求体生效（在协议翻译之后、
凭证选择之前），因此无论客户端原本说的是 Claude、OpenAI 还是 Gemini，
只要最终解析出的上游是 Anthropic 系通道，它们都会生效。

实现：[`sdk/gproxy-provider/src/utils/claude_cache_control.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/utils/claude_cache_control.rs)。

## 缓存断点规则

一条规则声明了"在哪里"打 `cache_control` 标记、"用哪种" TTL。每个供应商
最多携带 **4 条**规则 —— 与 Anthropic 的 4 断点上限对齐。

```jsonc
{
  "cache_breakpoints": [
    { "target": "tools",    "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "system",   "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "messages", "position": "last_nth", "index": 1, "ttl": "5m"   },
    { "target": "messages", "position": "last_nth", "index": 3, "ttl": "5m"   }
  ]
}
```

### 字段

| 字段 | 取值 | 含义 |
| --- | --- | --- |
| `target` | `top_level`（别名 `global`）\| `tools` \| `system` \| `messages` | 断点打在 Claude 请求体的哪一段。 |
| `position` | `nth`（默认）\| `last_nth` / `last` / `from_end` | 从头数还是从尾数。 |
| `index` | 正整数，默认 `1` | 1-based 下标。`1` + `last_nth` 即"最后一个"。 |
| `ttl` | `auto`（默认）\| `5m` \| `1h` | 缓存存活时间。`auto` 不写 ttl 字段，走 Anthropic 默认值 (5 分钟)。`1h` 需要 `extended-cache-ttl-2025-04-11` beta。 |

### 目标位置 (target)

- **`tools`** —— 断点打在 `tools` 数组里。适合工具列表较长且每轮稳定
  的场景。
- **`system`** —— 断点打在 `system` 内容数组里。适合长系统提示。
- **`messages`** —— 断点打在 `messages` 数组的某个内容块上。适合对话
  开头钉住的长上下文。
- **`top_level`** / **`global`** —— 为未来的顶层缓存 hook 预留；当前
  这类规则会被解析并保留，但实际上会把标记放在请求的顶层。

### 位置语义

`position: "nth"` 从前往后数，`position: "last_nth"` 从后往前数。两者都
是 1-based：

- `{ target: "messages", position: "nth", index: 1 }` —— 第一条消息。
- `{ target: "messages", position: "last_nth", index: 1 }` —— 最后一条消息。
- `{ target: "messages", position: "last_nth", index: 3 }` —— 倒数第三条消息。

如果下标越界，该条规则会被静默丢弃 —— GPROXY 绝不会留下悬空的
`cache_control`。

### 与客户端自带断点的组合

如果客户端已经在某些块上设置了 `cache_control`，GPROXY 会先把它们数进去，
然后**只填补剩余的槽位**（总上限仍是 4 个）。客户端设的内容一概不覆盖。
规范化阶段还会把字符串形态的 `system` / `content` 转成等价的
数组块 (array-of-blocks) 形态，这样规则才有具体的块可以挂载上去。

## 魔法字符串触发器

有时你没法改服务端配置，只能把缓存意图写进提示文本本身。
`settings.enable_magic_cache = true` 就是这种模式：客户端在某个文本块末尾
写一个约定好的标记字符串 (sentinel)，GPROXY 把该块改写为带
`cache_control: { type: "ephemeral", ttl: "..." }` 的块，并把标记字符串
从文本里擦掉。

识别的三种标记（常量定义在 `claude_cache_control.rs`）：

| 标记字符串 | 写入的 TTL |
| --- | --- |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH` | `auto`（默认） |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF` | `5m` |
| `GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY` | `1h` |

同样受 4 个断点的上限约束 —— 服务端会先数一数请求里已有的 `cache_control`
数量，再用剩余的槽位来填。

典型用法：某个只能动态拼提示、改不动服务端配置的工具 —— 知道标记字符串
的客户端库在需要的位置把它插进去，GPROXY 在转发前把它擦掉，上游最终看到
的就是一个"恰好在正确位置"带 `cache_control` 的普通请求。

## `auto` vs. 显式 TTL

- **`auto`** —— GPROXY 写 `{ "type": "ephemeral" }`，不带 ttl 字段。
  Anthropic 默认走 **5 分钟**缓存，这是最安全的选择，也不需要任何 beta。
- **`5m`** / **`1h`** —— GPROXY 显式写出 TTL。对于 `1h`，Anthropic 要求
  请求带上 `extended-cache-ttl-2025-04-11` beta 请求头。如果你用了任何
  `ttl: "1h"` 的规则，务必确保该 beta 被加上 —— 要么让客户端自己设置，
  要么把它列进 `settings.extra_beta_headers`，让 GPROXY 自动合并到每一次
  请求。

## 实战配方

### 缓存长系统提示和工具列表

```jsonc
{
  "cache_breakpoints": [
    { "target": "tools",  "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "system", "position": "last_nth", "index": 1, "ttl": "auto" }
  ]
}
```

每次请求的工具定义和系统提示都完全一样时用这个 —— 这是 agent 循环的
典型形态。

### 缓存对话开头钉住的长上下文

```jsonc
{
  "cache_breakpoints": [
    { "target": "messages", "position": "nth", "index": 1, "ttl": "5m" }
  ]
}
```

第一条用户消息（通常是一大段粘贴进来的文档或钉住的上下文窗口）就成为
缓存前缀。后续轮次只按缓存读取费率付费。

### 同时缓存"温热前缀"和"倒数第二条消息"

```jsonc
{
  "cache_breakpoints": [
    { "target": "tools",    "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "system",   "position": "last_nth", "index": 1, "ttl": "auto" },
    { "target": "messages", "position": "nth",      "index": 1, "ttl": "5m"   },
    { "target": "messages", "position": "last_nth", "index": 2, "ttl": "5m"   }
  ]
}
```

4 个断点全用上。工具列表 + 系统提示是"跨每次调用的温热前缀"；第一条消息
是钉住的长上下文；倒数第二条消息是一轮你预期会在紧凑循环里重放的稳定对话。

### 让客户端无感地启用 1 小时 TTL

```jsonc
{
  "cache_breakpoints": [
    { "target": "system", "position": "last_nth", "index": 1, "ttl": "1h" }
  ],
  "extra_beta_headers": [
    "extended-cache-ttl-2025-04-11"
  ]
}
```

`extra_beta_headers` 会合并到每一次上游请求，客户端根本不需要知道这个
beta —— GPROXY 会在转发前自动加上。

## 注意事项

- **4 个断点是硬上限。** 客户端自带的 `cache_control` 块也要计入这 4 个
  名额，如果客户端已经自己标了断点，你这边规则可用的槽位会比表面上看起来
  更少。
- **缓存 key 对内容敏感。** Anthropic 是按前缀的精确内容算 key 的。如果你
  同时配了 [请求改写规则](/zh-cn/guides/rewrite-rules/)去改动被缓存前缀
  里的文本，每次请求都会让缓存失效。改写要么放在缓存前缀碰不到的字段上，
  要么把断点放在被改写内容*之后*。
- **`claudecode` 对比 `anthropic`。** 两者的缓存能力集完全相同；
  `claudecode` 额外支持 `prelude_text` 设置，可以注入一段组织级的系统
  内容块。如果同时用到 prelude 和 `system` 断点，断点仍然会落在 prelude
  注入之后的正确位置 —— 它是在最终规范化过的请求体上计算的。
- **最小可缓存长度。** Anthropic 对可缓存前缀有最小长度限制；太短的
  提示即使带了断点也不会真的进缓存。当前阈值请参考 Anthropic 的
  [提示缓存文档](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)。
