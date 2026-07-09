---
title: 请求改写规则
description: 通过可复用规则添加系统指令、缓存断点、字段修改、文本替换和请求头。
---

当发往上游的请求需要统一调整时，可以使用可复用的 **Rule Set**。同一个规则集可以绑定到
多个 Provider，不需要为每个上游重复配置。

规则看到的是上游 API 格式。例如，OpenAI 请求被路由到 Claude 时，会先转换成 Claude
Messages，再执行绑定到该 Provider 的规则。

```text
client request
  -> 鉴权并选择上游
  -> 转换成上游 API 格式
  -> 应用 Provider Rule Set
  -> 发送上游请求
```

控制台中可以创建命名规则集、添加有序规则，再把规则集绑定到 Provider。禁用或无效的规则会
被跳过，不会让正常请求失败。

## 通用字段

每条规则包含：

- `kind`：`system_text`、`cache_breakpoint`、`rewrite`、`transform`、`header` 之一。
- `config_json`：该规则类型的具体配置。
- `filter_model_pattern`：可选 glob，匹配去掉前缀后的上游 model 名称。
- `filter_operation_keys`：可选 Operation 列表，例如 `generate_content` 或 `stream_generate_content`。
- `sort_order` 和 `enabled`。

过滤条件按 AND 组合。省略的维度表示匹配全部。

## `cache_breakpoint`

`cache_breakpoint` 会根据目标上游格式插入原生缓存断点。Claude 使用 `cache_control`，OpenAI
Chat 和 Responses 使用 `prompt_cache_breakpoint`。

```json
{
  "target": "system",
  "ttl": "30m"
}
```

OpenAI 支持 `system`、`last_message`，以及通过 `top_level` 设置请求级缓存行为；不支持在
`tools` 上添加断点。Claude 支持 `top_level`、`system`、`tools` 和 `last_message`。完整的
Target、TTL 和魔法字符串行为见[提示缓存](/zh-cn/guides/claude-caching/)。

## `rewrite`

`rewrite` 修改 JSON body path：

```json
{
  "path": "stream_options.include_usage",
  "action": "set",
  "value_json": true
}
```

支持的 action：

| Action | 行为 |
| --- | --- |
| `set` | 创建缺失的 object parent，并在 leaf 写入 `value_json`。 |
| `delete` | 删除存在的 object key 或 array element；缺失路径跳过。 |
| `merge` | 把 object 类型的 `value_json` shallow-merge 到路径上的现有 object。 |

路径使用点分隔，支持对象字段和数字数组索引，例如 `messages.0.content`。路径不存在时会
跳过，不会中断请求。

## `transform`

`transform` 对匹配的 JSON path 或序列化后的 body 做通用文本替换：

```json
{
  "phase": "request",
  "locate": { "match": "\\binternal-tool\\b" },
  "actions": [{ "op": "replace_text", "with": "tool" }]
}
```

结构化值可使用 `locate.path`，路径支持点分隔、`*` 通配符和可选的精确 `from` guard：

```json
{
  "phase": "response",
  "locate": { "path": "content.*.name" },
  "actions": [{ "op": "replace_text", "from": "tasklist", "with": "todowrite" }]
}
```

`phase` 可为 `request`、`response` 或 `both`，默认是 `request`。Regex 是序列化后的宽泛匹配，因此 pattern 要尽量精确。

## `header`

`header` 设置或合并请求 header：

```json
{
  "name": "anthropic-beta",
  "value": "extended-cache-ttl-2025-04-11",
  "mode": "merge"
}
```

`override` 会替换请求头。`merge` 会用逗号追加并去重，适合 `anthropic-beta` 这类请求头。

## 固定执行顺序

规则按固定顺序执行，不完全按绑定顺序：

```text
system_text -> cache_breakpoint -> rewrite -> transform -> header
```

同一类型内会保留规则集和规则的顺序。无法应用的规则会被跳过并记录日志，不会让上游请求
失败。
