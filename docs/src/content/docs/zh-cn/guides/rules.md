---
title: 路由规则与规则集
description: "规则集在 Provider 原生格式上修改请求与响应；路由规则决定每个 Provider 如何处理某个操作与入站协议"
---

GPROXY 有两套规则机制。两者都在控制台中编辑，也都保存在控制面里：

- **规则集**是可复用、有序的变更规则列表。一个规则集可以附加到一个或多个
  Provider，在请求发出之前修改 Provider 原生格式的请求，在响应转换回客户端
  格式之前修改 Provider 原生格式的响应。
- **路由规则**属于单个 Provider。它按操作和入站协议声明该 Provider 是直通
  请求、变换为另一种线上格式、在本地作答，还是拒绝处理。

全局 **规则** 工作区（`/admin/rules`）编辑规则集及其附加关系。Provider 详情
的 **规则** 标签页编辑同一批对象，但限定在该 Provider 范围内并提供预设；
**路由规则** 标签页编辑路由规则。

## 规则的执行位置

```text
client request
  -> classify operation and inbound protocol
  -> select provider and credential
  -> routing rule: passthrough / transform_to / local / unsupported
  -> protocol transform to the provider's native format
  -> rule sets, request phase (system_text -> cache_breakpoint -> rewrite
     -> transform -> header)
  -> Claude magic-string cache pass (if enabled on the provider)
  -> channel prepare: URL, auth, forwarded metadata
upstream response
  -> channel shaping
  -> rule sets, response phase (transform only), on 2xx bodies
  -> protocol transform back to the client's format
```

因此规则看到的是上游的线上格式。被路由到 Claude Provider 的 OpenAI Chat 请求
会先转换为 Claude Messages，规则编辑的是 `system`、`messages[]` 和
`tools[]`，而不是 `messages[].role`。

流式响应逐帧编辑：每个 SSE `data:` 载荷或 Gemini JSON 数组元素独立重写。
`[DONE]` 哨兵保持原样。响应规则不作用于 WebSocket 会话。

## 规则集

规则集有 `name`、可选的 `description` 和 `enabled` 标志。被禁用的规则集、规则
和附加关系都在编译时跳过。配置无法编译的规则在保存时即被拒绝，因此已保存的
规则不会让请求失败。

### 规则字段

每条规则以如下形状写入 `POST /admin/api/rules`：

```json
{
  "rule_set_id": 4,
  "config": { "kind": "system_text", "text": "...", "position": "prepend" },
  "filter_model_pattern": "claude-*",
  "filter_operations": ["generate_content", "stream_generate_content"],
  "filter_header_pattern": "^user-agent: opencode/",
  "sort_order": 0,
  "enabled": true
}
```

| 字段 | 含义 |
| --- | --- |
| `config.kind` | `system_text`、`cache_breakpoint`、`rewrite`、`transform` 或 `header`；`config` 的其余字段取决于类型。 |
| `filter_model_pattern` | Glob（`*`、`?`），匹配完整的上游模型 id；当客户端请求的名称（路由、别名或变体）与之不同时，也会匹配该名称。 |
| `filter_operations` | 稳定的操作 id，例如 `generate_content`、`stream_generate_content`、`list_models`、`create_embedding`。 |
| `filter_header_pattern` | 大小写不敏感的正则，逐行匹配渲染为 `name: value` 的入站请求头；任一行命中即生效。 |
| `sort_order` | 规则集内的声明顺序。 |

过滤条件按 AND 组合；省略的条件匹配全部。请求头过滤是把应用兼容性规则集限定
到单个客户端的手段：否则一条为 OpenCode 改写工具调用名的响应规则，会对共用该
Provider 的所有客户端生效。客户端实际发送的请求头行可在 **请求审计** 中查看
（见[用量、日志与审计](/zh-cn/guides/observability/)）。

### `system_text`

在目标格式的原生系统指令位置前置或追加服务端管理的文本。对非内容生成的操作
不做任何事。

```json
{ "kind": "system_text", "text": "Follow the workspace policy.", "position": "prepend" }
```

| 目标格式 | 位置 |
| --- | --- |
| Claude Messages | `system` 字符串，或插入 `system[]` 的文本块 |
| OpenAI Chat Completions | 一条 `role: "system"` 消息，置于最前或已有的开头系统消息之后 |
| OpenAI Responses（HTTP 与 WebSocket） | `instructions` |
| Gemini GenerateContent | `systemInstruction.parts[]` |

### `cache_breakpoint`

放置原生缓存标记：Claude 的 `cache_control`，或 OpenAI 的
`prompt_cache_breakpoint` / `prompt_cache_options`。Gemini 目标会跳过。

```json
{ "kind": "cache_breakpoint", "target": "system", "index": null, "ttl": "1h" }
```

目标可为 `top_level`、`system`、`tools`（仅 Claude）和 `message`。位置、TTL
和标记上限见[提示缓存](/zh-cn/guides/claude-caching/)。

### `rewrite`

编辑请求体中的一个 JSON 路径。路径用点分隔；每段是对象键或数字数组索引
（`messages.0.content`）。

```json
{ "kind": "rewrite", "path": "stream_options.include_usage", "action": "set", "value": true }
```

| 动作 | 行为 |
| --- | --- |
| `set` | 创建缺失的对象父级并在叶子写入 `value`；数组索引必须已存在。 |
| `delete` | 删除键或数组元素；路径缺失时跳过。 |
| `merge` | 把对象类型的 `value` 浅合并到路径上已有的对象。 |

控制台的模型变体编辑器把变体存成 `rewrite`/`set` 规则，模型过滤即变体名，
因此思考等级之类的变体就是一条可以直接查看的规则。

### `transform`

对选中的 JSON 字符串值或序列化后的请求体做文本替换。它是唯一具有响应阶段的
类型。

```json
{
  "kind": "transform",
  "phase": "request",
  "locate": { "type": "paths", "value": ["tools.*.name", "tool_choice.name"] },
  "actions": [{ "op": "replace_regex", "pattern": "^mcp_([^_].*)$", "with": "mcp__$1" }],
  "limit": null
}
```

| 字段 | 取值 |
| --- | --- |
| `phase` | `request`（默认）、`response`、`both` |
| `locate` | `{"type":"path","value":"a.*.b"}`、带 `*` 通配符的 `{"type":"paths","value":[...]}`，或对序列化请求体匹配的 `{"type":"match","value":"<regex>"}` |
| `actions[].op` | `replace_text`：`with` 加可选的精确 `from` 守卫；`replace_regex`：Rust 正则 `pattern` 与 `with`（支持 `$1` 分组） |
| `limit` | 最多匹配的值数量（路径定位）或替换次数（请求体匹配） |

请求体 `match` 定位只接受 `replace_text`；在流上它逐帧运行，所以要把模式
收窄并使用单词边界。

### `header`

设置或合并一个出站请求头。

```json
{ "kind": "header", "name": "anthropic-beta", "value": "extended-cache-ttl-2025-04-11", "mode": "merge" }
```

`override`（默认）替换请求头。`merge` 以逗号追加值，已存在时跳过。请求头规则
在 Provider 的转发元数据策略之前运行，因此该请求头必须是通道会转发的
（**Provider → 设置 → 转发元数据**）。

### 固定执行顺序

```text
system_text -> cache_breakpoint -> rewrite -> transform -> header
```

顺序先按类型决定，与规则来自哪个规则集无关。同一类型内，已附加的规则集按附加
顺序运行，规则按 `sort_order` 运行。控制台在每条规则旁显示得出的"实际第 N
个"。

### 把规则集附加到 Provider

附加关系（`POST /admin/api/provider-rule-sets`）把规则集与 Provider 关联，并带
有自己的 `sort_order` 和 `enabled` 标志。规则工作区按附加数量把规则集标为
**未使用**、**一个 Provider**（私有）或 **共享**。规则集仍有规则或附加关系时
不能删除。

创建 Provider 时会同时创建并附加一个名为 `<provider> · defaults` 的空私有规则
集。控制台把模型变体规则写在这里；你也可以往里添加自己的规则。

### 预设

`GET /admin/api/rule-presets` 列出预设；Provider 的规则标签页通过 **应用兼容性
预设** 应用某个预设（`POST /admin/api/providers/<id>/rule-presets/<preset>`）。
应用会创建或更新一个名为 `<preset> compatibility` 的普通规则集，附加到该
Provider，并保持可编辑。再次应用会就地刷新规则。

| 预设 | 类别 | 作用 |
| --- | --- | --- |
| OpenCode | application | 从 `system` 中去掉 `<env>` 块和 OpenCode 品牌；请求方向把小写工具名改为 Claude Code 的 TitleCase、`mcp_` 改为 `mcp__`，响应方向改回。限定 `^user-agent: opencode/`。 |
| pi-mono | application | 改写提示文本中的 "pi" 代理品牌。无请求头过滤。 |
| Aider | application | 改写 "Aider" 品牌；限定 `^user-agent: litellm/`。 |
| Cline | application | 改写 "Cline"；限定 Cline 的 user agent、`x-title` 或 `http-referer`。 |
| Continue | application | 改写 "Continue"；限定 `^user-agent: continue/`。 |
| Cursor | application | 改写 "Cursor"；限定 `^user-agent: cursor/`。 |
| Claude system cache | cache | 在最后一个 `system` 块上放 `cache_breakpoint`，TTL `1h`。 |
| Claude message cache | cache | 在最后一个可缓存消息块上放 `cache_breakpoint`，TTL `1h`。 |

应用类预设过滤 `generate_content` 和 `stream_generate_content`，且只作用于请求
文本路径。

## 路由规则

路由规则属于单个 Provider，以 `operation` 和入站 `kind` 为键；后者是线上格式
家族或内容生成协议，例如 `openai`、`claude`、`gemini`、`openai_chat`、
`openai_responses`、`openai_responses_websocket`、`claude_messages`、
`gemini_generate_content`。

```json
{
  "provider_id": 3,
  "operation": "generate_content",
  "kind": "claude_messages",
  "implementation": "transform_to",
  "dest_operation": "generate_content",
  "dest_kind": "openai_chat",
  "sort_order": 0,
  "enabled": true
}
```

| `implementation` | 效果 |
| --- | --- |
| `passthrough` | 以入站格式发送请求；通道必须原生支持该格式。 |
| `transform_to` | 在通道看到请求之前转换为 `dest_operation` + `dest_kind`，并把响应转换回来。两个 `dest_*` 字段都必填，且必须是已接线的变换对。 |
| `local` | GPROXY 用自身状态作答（模型列表、模型详情、Token 计数），不会为该操作调用这个 Provider。 |
| `unsupported` | 该 Provider 拒绝以此协议处理该操作。 |

### 通道默认值与操作员行

每个通道在代码里声明一张默认表。创建 Provider 时按表中每一项播种一行，来源为
`channel_default`；控制台把这些行显示为灰色并标注 **继承**。编辑、新增或删除
某行会把它变成操作员行。启动时会为已有 Provider 回填新增的通道默认值，不触碰
已存在的行。

**重置默认**（`POST /admin/api/providers/<id>/routing-defaults/reset`）删除该
Provider 的全部路由规则并重新播种通道表。

### "不支持"的含义

当请求解析到的规则为 `unsupported` 或 `local`，或者通道根本没有为该入站
（操作，协议）声明路由时，这个 Provider 不是有效目标：执行器跳过它并转到下一
个路由成员；如果路由中没有任何成员能处理，客户端收到不支持操作的错误。
`enabled: false` 的规则被忽略，改为采用通道自身的声明。
