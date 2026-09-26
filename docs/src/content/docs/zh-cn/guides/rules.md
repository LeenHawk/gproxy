---
title: 改写规则与操作覆盖
description: 对请求与响应载荷、header 与 query 值的有序正则替换：怎么过滤、怎么排序、怎么应用，以及按 Provider 的操作覆盖。
---

v4 在上游之前有两套运维者可编辑的机制，它们做不同的事。

- **改写规则**改字节：对选中的 JSON 字符串值、某个具名 header 值或某个具名 query 值的一条
  有序正则替换，两个方向都可以。
- **操作规则与操作端点**改*讲哪种方言*和*发到哪*：对某个操作上渠道所声明方言的按 Provider
  覆盖，以及对它所调用 URL 的覆盖。

v3 的五种规则 kind——`system_text`、`cache_breakpoint`、`rewrite`、`transform`、`header`
——塌进了下面这一种形状。一条规则现在就是一个 pattern、一个 replacement、一个应用位置和
一组过滤器，没有别的。

## 它们在哪里运行

```text
客户端请求
  → 解析目标 · 准入调用方
  → 方言不同时转换成 Provider 的原生方言
  → 改写规则，request 阶段
  → 渠道：URL、鉴权、它自己的 header
上游响应
  → 改写规则，response 阶段（流式则按流单元）
  → 转换回客户端的方言
```

因此规则见到的是**上游的 wire 形状**，不是客户端的。一个被路由到 Claude Provider 的
OpenAI Chat 请求会先转成 Claude Messages，所以它的规则寻址的是 `system`、`messages[]`
和 `tools[]`。

## 规则集

一个规则集是名字、可选描述和一个 `enabled` 标志。它被挂到一个或多个 Provider 上；一次挂载
有自己的 `sortOrder` 和 `enabled`。

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/rule-sets \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"name":"demo"}'

curl -s -X POST http://127.0.0.1:8787/admin/api/provider-rule-sets \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","ruleSetId":"…","sortOrder":0}'
```

整套规则可以一次替换，这也是把规则集当作一个整体来编辑而不是逐行改的方式：

```sh
curl -s -X PUT http://127.0.0.1:8787/admin/api/rule-sets/{id}/rules \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '[ … ]'
```

## 一条规则

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/rules \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"ruleSetId":"…","phase":"request","target":"body",
       "paths":["messages.*.content"],
       "pattern":"(?i)\\bwidget\\b","replacement":"gadget",
       "filterOperationKeys":[{"operation":"generate_content","dialect":"openai_chat"}]}'
```

```json
{"id":"de487987ff3d2e6415986ca9099dd60d","ruleSetId":"f41160b45f36…",
 "phase":"request","target":"body","targetName":null,
 "paths":["messages.*.content"],"pattern":"(?i)\\bwidget\\b","replacement":"gadget",
 "filterOperationKeys":[{"dialect":"openai_chat","operation":"generate_content"}],
 "filterModelPattern":null,"filterHeaderPattern":null,"filterEventPattern":null,
 "sortOrder":0,"enabled":true,…}
```

| 字段 | 含义 |
| --- | --- |
| `phase` | `request`、`response` 或 `both`，**相对上游连接**而言。 |
| `target` | `body`、`header` 或 `query`。 |
| `targetName` | `header` 与 `query` 必填，`body` 必须不填。header 名大小写不敏感，query 名解码后精确匹配。 |
| `paths` | 只对 body。一个点路径数组，选中 JSON *字符串值*；`null` 表示对整份载荷文本应用。 |
| `pattern` | Rust 正则语法，包括 `(?i)`、`(?s)` 这类内联标志。 |
| `replacement` | 正则替换语法，包括 `$1` 与 `${name}`。 |
| `sortOrder` | 集合内升序；id 破平。 |

pattern 编译不过的规则在**保存时**就被拒绝，所以已存的规则不会在请求里失败。正则和路径
每个配置 revision 只编译一次；执行期绝不重新校验一行数据。

### 路径

`messages.*.content`、`system`、`system.*.text`、`tools.*.name`。一段是对象键、数组下标，
或表示"每个元素"的 `*`。

载荷是**被扫描的，不是被解析成树的**，所以键序、空白和数字格式原样保留，只有选中的字符串
被重新编码。路径规则作用在非 JSON 载荷上是空操作，不是错误。

`paths: null` 时 pattern 对序列化后的 body 文本运行。这种 pattern 要写窄、要加词边界——
在流上它对每个单元都跑一遍。

### header 与 query 值

一条 header 规则替换该 header **每一个**值的内部，重复值的数量与顺序保持不变。query 规则
对一个参数做同样的事，而且未被触碰的原始片段逐字节保留——只有被改写的值会重新编码。

值不是文本的 header 是错误，绝不做有损解码。

## 过滤器

所有过滤器是与的关系；省略的过滤器匹配一切。

| 过滤器 | 匹配什么 |
| --- | --- |
| `filterOperationKeys` | `{"operation": …, "dialect": …}` 对的 JSON 数组。指的是转换之后、对该 Provider 执行的**原生**操作。 |
| `filterModelPattern` | 对选中的上游模型**或**调用方请求的名字做 `*` / `?` glob——任一侧命中即算命中。 |
| `filterHeaderPattern` | 对**入站**请求 header 行做大小写不敏感的正则。 |
| `filterEventPattern` | 对 SSE 事件名做正则（退化到 JSON `type`）；WebSocket 则是 JSON `type`。 |

header 过滤器正是把一套兼容规则收窄到一个客户端的手段：否则一条为某个编辑器重命名工具调用
的响应规则，会对共用这个 Provider 的每个客户端都生效。入站 header 是过滤**条件**，绝不是
改写目标——改 header 的规则改的是发往上游的那一个。

事件过滤器需要解码后的单元，因此在单元到达时才求值，而不是在选规则时。

## 顺序

Provider 上启用的挂载按 `(sortOrder, id)` 顺序运行，每个集合内部的规则也按同样顺序。
**可见顺序就是执行顺序，而且每条规则都看到上一条的输出。** 不存在 v3 那种"先按 kind 排序"
的做法；列表里看到什么就发生什么。

## 流式

流式响应**按单元**改写——一个完整的 SSE 事件、一个 JSON 数组元素，或一条 NDJSON 记录。
单元绝不是网络 chunk，而且不会有任何东西把活的流缓冲到结束。

除非有规则改了它的载荷，帧逐字节通过。SSE 只替换 `data:` 行，所以注释、`event:`、`id:`、
`retry:` 行和 `[DONE]` 哨兵原样保留。超过 `settings.maxStreamEventBytes`（默认 32 MiB）的
单元是错误，而不是无界缓冲。

## 预设

内置六套应用兼容预设。每一套都让某个客户端应用在识别它的上游面前看起来像一个通用客户端。

```sh
curl -s http://127.0.0.1:8787/admin/api/rule-presets -H "Authorization: Bearer $GPROXY_KEY"
```

```text
opencode    OpenCode        application   37 条规则
the agent   the agent-mono  application    9 条规则
aider       Aider           application    2 条规则
cline       Cline           application    1 条规则
continue    Continue        application    1 条规则
cursor      Cursor          application    1 条规则
```

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/rule-sets/{id}/rule-presets/opencode \
  -H "Authorization: Bearer $GPROXY_KEY"
```

应用预设是**替换**集合里的规则，而不是合并。一套预设是一个有序的整体答案，把它的一半和
别的东西交错起来，改写出来的文本谁也预测不了。想保留已有规则，就读 `rule-presets`、
自己合并列表，再 `PUT` 回去。

v3 那两套 cache 预设随 `cache_breakpoint` 规则 kind 一起消失了。v4 的做法见
[提示缓存](/zh-cn/guides/claude-caching/)。

## 操作覆盖

改写规则没法让一个 Provider 承接它不承接的操作，也没法把它发到别处。两张按 Provider 的表
可以。

**`operation_rules`** 覆盖某个操作上这个 Provider 原生讲哪些方言。渠道默认值留在**代码里**，
不会被拷进每个新建的 Provider——这就是与 v3 的差别：没有被种下的 `channel_default` 行要和
运维者的行区分，因为根本没有被种下的行。

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/operation-rules \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content",
       "action":"dialects","target":["claude","openai_chat"]}'
```

`action: "dialects"` 加上 `target` 里的方言 id 数组，替换渠道对该操作的声明。接下来发生
什么只取决于这个列表：

| 客户端的方言 | 会发生什么 |
| --- | --- |
| 在列表里 | 直通——字节不被转换 |
| 不在列表里 | 转换成**第一个**声明的方言，再转回来 |
| 不在列表里，且该操作这个 Provider 只提供缓冲版 | 转换、调用一次，再合成客户端自己的流 |
| 不在列表里，且该操作没有任何声明 | 这个 Provider 不是它的有效目标 |

客户端的方言只要是原生的就胜出；否则第一项是转换目标，因此那个数组的顺序就是偏好。

**`operation_endpoints`** 对某个 `(操作, 方言, 传输)` 整体替换方法 URL。它不是一个再拼
默认路径的 base URL——渠道自己的路径参数由那个方法解析：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/operation-endpoints \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content","dialect":"openai_chat",
       "url":"https://elsewhere.example/v1/chat/completions"}'
```

缺失或被禁用的行把 URL 构造留给 Provider 的 `baseUrl` 和渠道的默认路径。

两者由同一个重置一起清掉：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/providers/{id}/routing-defaults/reset \
  -H "Authorization: Bearer $GPROXY_KEY"
```

## `custom` 渠道自己的列表

对 `custom` 渠道，同一个问题还有第二个答案，而且它是一个 Provider `config` 键而不是操作
规则——因为一个 `custom` Provider 背后没有厂商，渠道无从知道它的方言：

```json
{ "config": { "dialects": ["openai_chat", "openai", "claude"] } }
```

方言既不在列表里、也无法通过转换到达的操作会失败，并报出两侧：

```text
models.list: no conversion from OpenAi to OpenAiChat
```

这是第一天最常见的意外。如果某个操作在 `custom` Provider 上失败，先看这个列表，再看别处。
