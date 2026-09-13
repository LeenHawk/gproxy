# gproxy-protocol

[English](https://github.com/LeenHawk/gproxy/blob/4.0/crates/gproxy-protocol/README.md) | 简体中文

面向 AI API 网关、SDK 和协议适配器的 Rust HTTP/WebSocket 类型库。
将 HTTP 元信息与原始或强类型 body 放在同一条消息中，独立建模已建立的 WebSocket 连接。

当前为 v4 开发版，提供模型发现、令牌计数、内容生成与流式、上下文管理、嵌入、重排、
文件、图像、音频、视频和实时会话的原生类型。HTTP 元信息与强类型 body 分离；
WebSocket 消息和双向连接分别建模。

已声明字段与变体依据对应的厂商 API 或官方客户端结构。`rest` 保留未知扩展；只有来源
明确允许任意 JSON 的位置才保留任意 JSON 类型。body 编解码器、网络客户端和跨格式转换尚未实现。

## 安装

v4 预发布版本对应的 Cargo 依赖为：

```toml
[dependencies]
gproxy-protocol = "4.0.0-dev"
```

该版本发布前，可使用仓库 `4.0` 分支的本地检出：

```toml
[dependencies]
gproxy-protocol = { path = "../gproxy/crates/gproxy-protocol" }
```

## HTTP 请求与响应

`WireRequest<B>` 包含 `method`、`path`、`query`、`headers`、`body` 五个要素。
`WireResponse<B>` 包含 `status`、`headers`、`body` 三个要素。
body 可以是原始 `HttpBody`、解析后的 multipart，也可以是厂商的 JSON body 类型。

```rust
use gproxy_protocol::connection::{HeaderMap, Method, StatusCode};
use gproxy_protocol::openai::models::{GetModelRequest, GetModelResponse, Model};

let request = GetModelRequest {
    method: Method::GET,
    path: "/v1/models/gpt-5".into(),
    query: None,
    headers: HeaderMap::new(),
    body: (), // 没有 HTTP body。
};

let body: Model = serde_json::from_str(r#"{
    "id": "gpt-5", "object": "model", "created": 0, "owned_by": "openai"
}"#).unwrap();
let response = GetModelResponse {
    status: StatusCode::OK,
    headers: HeaderMap::new(),
    body,
};

assert_eq!(request.path, "/v1/models/gpt-5");
assert_eq!(response.body.id, "gpt-5");
```

运行此示例需添加 `serde_json = "1"`。JSON 序列化/反序列化只作用于 body；
HTTP 方法、路径、查询参数、状态码和头始终在 body 外面。query 字符串保留原始编码、
参数顺序和重复键。

各厂商的 `CountTokensRequest`、`CountTokensResponse` 同样使用 HTTP 消息包装各自的
`CountTokensRequestBody`、`CountTokensResponseBody`。厂商定义在 JSON 中的 model
仍保留在 body；Gemini 外层的模型资源名放在请求路径中。

## 流、multipart 与 WebSocket

- `HttpBody::Bytes` 表示完整的已编码 body，`HttpBody::Stream` 表示原始字节块。
  字节块可能切在 UTF-8 字符、JSON 值、SSE 事件或 multipart 分隔符中间。
- `StreamFraming` 描述 SSE、增量 JSON 数组或 NDJSON 的分帧格式，只提供格式标识，
  不执行解析，也不假设网络 chunk 与记录边界对齐。
- `Multipart` 按顺序提供 parts；每个 `MultipartPart` 有自己的 headers 和完整/流式 body。
  相同字段名的多个 part 会被保留。
- WebSocket 握手使用 HTTP 请求和响应；已建立的 `WebSocket` 提供独立的接收流和发送端，
  支持文本、二进制、ping、pong、close。它不属于 HTTP body 变体，连接建立与底层帧处理由传输适配器负责。

## 内容生成的强类型载荷

| API | 请求与非流式响应 | 流式载荷 |
|---|---|---|
| Claude Messages | `claude::generate_content` | `claude::stream::StreamEvent` |
| Gemini | `gemini::generate_content` | `gemini::stream::StreamChunk` |
| OpenAI Responses | `openai::responses::{generate, response}` | `openai::responses::stream::StreamEvent` |
| OpenAI Chat Completions | `openai::chat::{request, response}` | `openai::chat::stream::ChatCompletionChunk` |

流式响应别名承载 `ByteStream`。调用方处理 SSE 或 JSON 数组分帧，再把每条 JSON 载荷
反序列化为对应类型。这些类型不负责拆解网络 chunk、累积模型输出或跨 API 转换；
Chat Completions 的 `[DONE]` 等非 JSON 传输标记也由调用方的分帧逻辑处理。

## 其他 API 模型

| 范围 | 模块 |
|---|---|
| 上下文与会话 | `openai::{compact, conversation, memory}` |
| Codex 客户端接口 | `openai::{guardian, web_search}` |
| 嵌入与重排 | `openai::{embeddings, rerank}`、`gemini::embeddings` |
| 文件操作 | `openai::files`、`claude::files`、`gemini::files` |
| 图像与音频 | `openai::{images, audio}` |
| 视频 | `openai::video`，扩展格式与原生格式独立 |
| Responses WebSocket | `openai::responses::websocket` |
| 实时 / WebRTC | `openai::realtime`、`gemini::live` |

multipart 表单包含明确的元数据字段和真实的 `MultipartPart` 文件流，下载响应保留原始 body。
类型本身不上传文件、不抓取结果 URL，也不在原生与扩展视频格式之间执行转换。

`spec::OPERATION_SPECS` 描述已建模的操作／方言组合及 wire 格式，不代表某个供应商支持该操作，
也不代表转换已经存在。TLS、ALPN 和 HTTP/2 客户端配置归出站 client 实现。

## 操作与方言

用 `OperationKey { operation, dialect }` 标识 API 操作及其数据格式。
键实现了 `Copy`、`Eq`、`Hash`、`Ord`，可用于映射表和集合。

| 方言 | 内容生成 API |
|---|---|
| `OpenAi` | OpenAI Responses |
| `Claude` | Anthropic Messages |
| `Gemini` | Gemini GenerateContent |
| `OpenAiChat` | OpenAI Chat Completions |
| `OpenAiResponsesWebSocket` | 通过 WebSocket 使用 OpenAI Responses |

方言表示格式，与实际托管模型的服务无关。构造键不会检查转换或后端是否支持它。

## 字符串标识

`Operation` 和 `Dialect` 支持字符串解析和稳定的字符串标识，Serde 序列化也使用相同的字符串：

```rust
use gproxy_protocol::{Dialect, Operation};

let operation: Operation = "generate_content".parse().unwrap();
let dialect: Dialect = "openai_chat".parse().unwrap();

assert_eq!(operation, Operation::GenerateContent);
assert_eq!(dialect, Dialect::OpenAiChat);
assert_eq!(dialect.id(), "openai_chat");
assert!("unknown".parse::<Dialect>().is_err());
```

## Features

默认不启用任何 feature。公开枚举带有 `non_exhaustive`，进行 `match` 时需包含通配分支。
`exhaustive` feature 会解除这一限制；启用后，升级版本时若枚举新增变体，可能需要更新匹配代码。

## 许可证

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html)。
