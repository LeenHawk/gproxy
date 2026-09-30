---
title: "重写规则与协议覆盖"
description: "设置提示词、缓存断点、JSON 与文本重写，以及上游协议和端点。"
---


在供应商的重写规则页面新增规则，选择类型并填写内容。新建供应商会自动创建并绑定同名默认规则集，一般不需要手动管理绑定；多个供应商需要共用规则时，再使用高级绑定设置。

## 规则类型

| 界面类型 | 用途 | API `action` |
| --- | --- | --- |
| 系统提示词 | 在现有系统指令前或后添加文本 | `system_text` |
| 缓存断点 | 按协议为全局、系统、消息或工具添加缓存标记 | `cache_breakpoint` |
| JSON 改写 | 设置、删除字段，或合并 JSON 对象 | `set`、`delete`、`merge` |
| 文本转换 | 对正文、请求头或查询参数做正则替换 | `replace` |
| 请求头 | 设置值，或合并逗号分隔的值 | `header_set`、`header_merge` |

系统提示词支持 Claude、OpenAI Chat、Responses（含 WebSocket）和 Gemini。缓存断点支持 Claude 与 OpenAI 协议；具体位置和有效期见[提示缓存](/zh-cn/guides/claude-caching/)。

## 高级 JSON 编辑

在规则集内新建或编辑规则时，切换到“高级 JSON”即可直接粘贴一个 v4 规则对象。供应商的重写规则页面也提供此入口。表单和 JSON 可以相互切换，并支持格式化；JSON 有误时会保留输入并提示，保存时还会校验规则是否可执行。

例如，将正文中的 `"type":"function"}` 替换为 `"type":"function","strict":false}`：

```json
{
  "action": "replace",
  "phase": "request",
  "target": "body",
  "pattern": "\"type\":\"function\"\\}",
  "replacement": "\"type\":\"function\",\"strict\":false}"
}
```

这里使用 v4 的 `action`、`pattern`、`replacement` 字段。规则集、ID 和顺序由页面管理，不填入 JSON；可选的过滤条件及 `enabled` 可以一并填写。省略过滤条件表示不限制匹配，省略 `enabled` 表示启用。一次编辑一条规则，不接受规则数组。

## 执行位置与顺序

请求先转换为上游协议，再执行请求重写，然后由渠道发送。响应先执行响应重写，再转换为客户端协议。因此，规则中的字段路径必须使用**上游格式**。

例如，OpenAI Chat 请求转到 Claude 供应商后，规则应使用 Claude 的 `system` 和 `messages` 字段。

规则集绑定和集内规则分别按 `sortOrder`、ID 排序，只执行启用项。后一条规则会看到前一条的结果。系统提示词、缓存断点和其他规则也遵循这个顺序。

## 字段与正则

JSON 改写需要填写路径，例如 `temperature` 或 `messages.*.content`。路径中的 `*` 表示所有元素。`set` 的值必须是合法 JSON，`merge` 的值必须是 JSON 对象；`delete` 不需要值。

文本转换使用 Rust 正则语法，支持 `(?i)`、`(?s)` 和替换中的 `$1`、`${name}`。填写正文路径时只替换匹配的 JSON 字符串值；不填路径时对整段正文文本替换。修改 JSON 文本时应避免破坏语法。

请求头和查询参数规则需要 `targetName`。`header_set` 替换请求头值；`header_merge` 合并逗号分隔的值并去重。不要把上游密钥写进可共享的规则。

## 过滤条件

| 条件 | 匹配内容 |
| --- | --- |
| 操作与协议 | 转换后在供应商上执行的操作 |
| 模型 | 上游模型或客户端请求的模型名，支持 `*`、`?` |
| 请求头 | 入站请求头的正则匹配 |
| 流事件 | SSE 事件名或 JSON `type`，用于正文规则 |

多个条件同时满足时规则才生效。省略的条件不限制匹配。为某个客户端添加兼容规则时，应配置请求头过滤，避免影响其他客户端。

## 流式响应

正文重写以完整事件为单位执行，例如 SSE 事件或 JSON 数组元素，不以网络数据块为单位。单个事件超过 `settings.maxStreamEventBytes` 时会报错。可以用事件过滤器只修改特定类型的事件。

## 通过 API 管理

下面创建一条正则替换规则，把指定路径中的 `widget` 替换为 `gadget`。将规则集 ID 替换为你的实际值。

```sh
curl -sS -X POST http://127.0.0.1:8787/admin/api/rules \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"ruleSetId":"your-rule-set-id","action":"replace",
       "phase":"request","target":"body","paths":["messages.*.content"],
       "pattern":"(?i)\\bwidget\\b","replacement":"gadget",
       "filterOperationKeys":[{"operation":"generate_content","dialect":"openai_chat"}]}'
```

`PUT /admin/api/rule-sets/{id}/rules` 可一次替换整组规则。系统提示词与缓存断点的 `replacement` 是 JSON 编码的配置字符串，控制台会生成它，不需要手工拼接。

## 兼容预设

`GET /admin/api/rule-presets` 返回内置的 OpenCode、the agent-mono、Aider、Cline、Continue 和 Cursor 兼容预设。规则数量与内容以当前接口返回为准。

`POST /admin/api/rule-sets/{id}/rule-presets/{preset}` 会**替换**规则集内的已有规则。需要保留原规则时，先导出或读取规则，合并后再保存。

## 协议与端点覆盖

供应商的“路由规则”控制使用哪个上游协议，与选择供应商的“模型路由”是不同的配置。

- `operation_rules` 覆盖某个操作支持的协议列表。客户端协议在列表中时优先直通，否则尝试转换到列表的第一项。
- `operation_endpoints` 覆盖某个操作、协议和传输方式的完整端点 URL。

例如，修改生成操作支持的协议：

```sh
curl -sS -X POST http://127.0.0.1:8787/admin/api/operation-rules \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"providerId":"your-provider-id","operation":"generate_content",
       "action":"dialects","target":["claude","openai_chat"]}'
```

没有覆盖时使用渠道默认值。`POST /admin/api/providers/{id}/routing-defaults/reset` 清除该供应商的操作规则和端点覆盖。

`custom` 渠道还需要在供应商配置中声明 `dialects`，例如 `["openai_chat", "openai"]`。某个操作无法调用时，先核对上游是否支持它，再检查这个列表和操作覆盖。
