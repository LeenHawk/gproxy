---
title: 路由表
description: v2 如何用持久化 routing_rules 表达 passthrough、transform、local 和 unsupported。
---

gproxy v2 的 routing table 是每个 provider 一份的持久化矩阵。每行把一个
入站 `(operation, kind)` 映射到一种实现：

- `passthrough`：不改变 wire dialect，直接转发给选中的 provider；
- `transform_to`：发送上游前转换到另一个 operation/kind，支持时再把响应转回；
- `local`：在 gproxy 内部处理，不调用上游；
- `unsupported`：拒绝该单元格。

请求时只读取已存储且启用的行。没有匹配行就视为 unsupported。channel
默认值只在创建 provider 或操作员重置 provider routing rules 时物化为真实
`routing_rules` 行。

## 存储行结构

`routing_rules` 行属于一个 provider：

| 字段 | 说明 |
| --- | --- |
| `provider_id` | 由哪个 provider 的 channel 和 credential 处理请求。 |
| `operation` | provider-neutral operation 字符串，例如 `generate_content`、`stream_generate_content`、`list_models`、`count_tokens`、`create_image` 或 `create_embedding`。 |
| `kind` | 入站 wire kind。content generation 使用具体 dialect：`open_ai_responses`、`open_ai_chat_completions`、`claude_messages`、`gemini_generate_content`。其它 operation 使用 provider family：`open_ai`、`claude`、`gemini`。 |
| `implementation` | `passthrough`、`transform_to`、`local` 或 `unsupported`。 |
| `dest_operation` | `transform_to` 的目标 operation；可为空，表示保持原 operation。 |
| `dest_kind` | `transform_to` 的目标 wire kind。transform 行缺少 `dest_kind` 时会被视为 unsupported。 |
| `sort_order` | 编译启用行时的顺序。实际唯一键仍是 `(provider_id, operation, kind)`。 |
| `enabled` | 禁用行会被忽略。 |

数据库对 `(provider_id, operation, kind)` 建唯一约束。

## Operation 词表

当前 operation enum 值：

| Operation | 分组 | 说明 |
| --- | --- | --- |
| `list_models`, `get_model` | Models | 模型列表/获取端点。 |
| `count_tokens` | Count tokens | provider token counting 端点。 |
| `generate_content`, `stream_generate_content` | Generate content | OpenAI Chat Completions、OpenAI Responses、Claude Messages、Gemini generateContent dialect。 |
| `create_image`, `edit_image` | Images | OpenAI-shaped 图片生成/编辑 operation；只有已实现的路径才可转换。 |
| `create_embedding` | Embeddings | OpenAI 和 Gemini embedding shape。 |
| `compact_content` | Compact | agent 工作流使用的 compact endpoint。 |
| `create_conversation` | Conversation | OpenAI conversation-shaped operation。 |

content-generation operation 必须使用 content-generation kind。非 content
operation 使用 provider family kind。

## 示例行

把 OpenAI Responses 流量 passthrough 到 OpenAI provider：

```json
{
  "provider_id": 1,
  "operation": "generate_content",
  "kind": "open_ai_responses",
  "implementation": "passthrough",
  "dest_operation": null,
  "dest_kind": null,
  "sort_order": 0,
  "enabled": true
}
```

接受客户端 Claude Messages，并转换成 OpenAI Responses 发给上游：

```json
{
  "provider_id": 1,
  "operation": "generate_content",
  "kind": "claude_messages",
  "implementation": "transform_to",
  "dest_operation": "generate_content",
  "dest_kind": "open_ai_responses",
  "sort_order": 10,
  "enabled": true
}
```

在本地回答模型列表：

```json
{
  "provider_id": 1,
  "operation": "list_models",
  "kind": "open_ai",
  "implementation": "local",
  "dest_operation": null,
  "dest_kind": null,
  "sort_order": 20,
  "enabled": true
}
```

## 默认 seed 与 reset

每个 channel 在代码中暴露默认 routing table。通过 admin API 创建 provider
时，会把该 channel 声明的单元格 seed 到 `routing_rules`。
reset endpoint 会按 provider 的 channel 重新计算默认值：

```text
POST /admin/providers/{provider_id}/routing-rules/reset
```

reset 会覆盖 channel 声明的默认单元格。它不会为 channel 未声明的单元格凭空增加支持，也不会删除默认表外的 operator 自定义行。

原始 JSON bundle import 不会调用 provider 创建 helper。如果 bundle 需要
routing 行，请显式包含 `routing_rules` 数组，或导入后通过 admin API reset provider。

## 请求流程

1. HTTP path 被分类成 `OperationKey`。
2. route 或 scoped-provider 选择 candidate provider 和上游 model id。
3. 编译该 provider 启用的 `routing_rules`。
4. 应用 dispatch decision：
   - `passthrough`：保留入站 request target/body shape；
   - `transform_to`：合成目标 provider-relative target，并运行 transform 层；
   - `local`：调用 channel local handler；
   - 缺行或 `unsupported`：返回不支持的 operation 错误。

## Routes 与 routing rules 的区别

`routes` 和 `route_members` 决定一个逻辑模型名由哪个 provider/model
candidate 处理。`routing_rules` 决定被选中的 provider 是否能处理该入站
wire operation，以及请求要如何改形。

例如，alias 可以把 `default-chat` 解析到 route `main`，route `main`
选择 provider `openai-main` 的 `gpt-4.1-mini`。随后该 provider 的
`routing_rules` 决定 Claude Messages 请求是否能转换成 OpenAI 上游 dialect。
