---
title: 路由表 (Dispatch Table)
description: 每个通道如何声明它响应哪些 (operation, protocol) 组合 —— 以及是透传、翻译、本地处理还是拒绝。
---

**路由表 (dispatch table)** 是每个通道的路由映射。它接收一个被分类成
`(OperationFamily, ProtocolKind)` 对的请求，然后告诉 GPROXY 以下四件事之一：

- **`Passthrough` (透传)** —— 请求原样转发到上游（两端协议相同；GPROXY 只
  改写模型名、头和认证信封）。
- **`TransformTo { destination }` (翻译)** —— 进入协议 `transform` 层，先把
  请求翻译成另一个 `(operation, protocol)` 再转发。响应走反向翻译。
- **`Local` (本地处理)** —— 请求完全在 GPROXY 内部处理，永远不打上游。
  用于 `*-only` 预设下的 `model_list` / `model_get`。
- **`Unsupported` (不支持)** —— 当前通道不支持该路由，返回 501。

每个通道都有 `fn dispatch_table(&self) -> DispatchTable` 方法。该方法返回
的是通道的**默认**路由表 —— 除非操作员在控制台里按供应商粒度覆盖，否则
引擎直接使用它。

定义在 [`sdk/gproxy-provider/src/dispatch.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/dispatch.rs)。

## Route key

```rust
pub struct RouteKey {
    pub operation: OperationFamily,
    pub protocol:  ProtocolKind,
}
```

### Operation family

`OperationFamily` (定义在 `gproxy-protocol`) 是路由代表的那种调用的
协议无关种类。完整列表：

| Family | 含义 |
| --- | --- |
| `model_list` | 列出模型。 |
| `model_get` | 按 id 取单个模型。 |
| `count_tokens` | token 计数接口。 |
| `compact` | "压缩"对话 (Claude Code compact)。 |
| `generate_content` | 非流式 chat / message / generate 调用。 |
| `stream_generate_content` | 流式 chat / message / generate 调用。 |
| `create_image` / `stream_create_image` | 图片生成。 |
| `create_image_edit` / `stream_create_image_edit` | 图片编辑。 |
| `openai_response_websocket` | OpenAI Responses WebSocket。 |
| `gemini_live` | Gemini Live 双向会话。 |
| `embeddings` | 向量接口。 |
| `file_upload` / `file_list` / `file_get` / `file_content` / `file_delete` | 文件接口。 |

### Protocol kind

`ProtocolKind` 是具体的 wire 方言：

| Kind | 含义 |
| --- | --- |
| `openai` | OpenAI —— 在选定具体 API 之前的 umbrella。 |
| `openai_chat_completions` | OpenAI **Chat Completions** API。 |
| `openai_response` | OpenAI **Responses** API。 |
| `claude` | Anthropic Messages API。 |
| `gemini` | Google Gemini `generateContent`。 |
| `gemini_ndjson` | Gemini NDJSON 流式 (语义上与 `gemini` 相同)。 |

## 示例：Anthropic 通道的默认路由表

精简自 [`channels/anthropic.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/channels/anthropic.rs)：

```rust
let pass  = |op, proto| (RouteKey::new(op, proto), RouteImplementation::Passthrough);
let xform = |op, proto, dst_op, dst_proto| (
    RouteKey::new(op, proto),
    RouteImplementation::TransformTo {
        destination: RouteKey::new(dst_op, dst_proto),
    },
);

let routes = vec![
    // 原生 Claude 路由 —— 透传。
    pass(OperationFamily::GenerateContent, ProtocolKind::Claude),
    pass(OperationFamily::StreamGenerateContent, ProtocolKind::Claude),

    // OpenAI Chat Completions 客户端 → 进入时翻译成 Claude。
    xform(
        OperationFamily::GenerateContent,   ProtocolKind::OpenAiChatCompletion,
        OperationFamily::GenerateContent,   ProtocolKind::Claude,
    ),
    // …
];
```

每一行的读法是：*"当**客户端**发来 `(operation, protocol)`，做 X"*。
`TransformTo` 里的 destination 是**上游**最终看到的 `(op, proto)` 对。

## 透传 vs. 翻译

两者的差别体现在热路径上：

- **`Passthrough`** 是最小解析的快路径。请求 body 原样转发，只动模型名、
  header 和认证 —— 同协议路由便宜就在这儿，详见
  [供应商与通道](/zh-cn/guides/providers/)。
- **`TransformTo`** 会走 `gproxy_protocol::transform` 里的协议转换，产出
  上游能接受的新 body。响应走反向转换（包括流式响应逐 chunk 改写）。

## `Local` 模型列表分发

`*-only` 预设（例如 `chat-completions-only` / `response-only` /
`claude-only` / `gemini-only`）会把 `model_list` 和 `model_get` 设为
`Local`。这类请求完全从本地 `models` 表响应 —— GPROXY 永远不会打上游。

具体而言，当通道的路由表把某条路由设为 `Local` 时，handler 会调用
`Channel::handle_local(...)`。该方法直接根据内存里的模型快照构造协议
对应的响应 body。

对于 `*-like` / 透传预设，`model_list` 仍然是 `Passthrough`，但 GPROXY 在
上游响应回来之后会与本地 `models` 表**合并**（原因与细节见
[模型与别名](/zh-cn/guides/models/)）。

## 操作员什么时候关心它

大多数用户根本不会动路由表 —— 通道自带的默认值已经够用。你会想覆盖它
通常是因为：

- **强制本地模型列表** —— 当某个上游返回一个巨大的模型列表，你不想把它
  暴露给客户端时。
- **禁用 (`Unsupported`)** 某个上游技术上支持但你不希望开放的能力
  （比如在一个 OpenAI 供应商上关掉图片生成）。
- **接线改造** —— 例如把 OpenAI Responses 的流量通过翻译重定向到一个
  Chat Completions 上游。

供应商级的覆盖可以在控制台的供应商工作区里编辑，以 JSON
(`DispatchTableDocument`) 形式保存。schema 限制每个
`(operation, protocol)` 对只能有一条规则。
