# gproxy-protocol

[English](https://github.com/LeenHawk/gproxy/blob/4.0/crates/gproxy-protocol/README.md) | 简体中文

面向 AI API 网关、SDK 和协议适配器的 Rust HTTP/WebSocket 类型库。
将 HTTP 元信息与原始或强类型 body 放在同一条消息中，独立建模已建立的 WebSocket 连接。

当前为 v4 开发版，包含连接模型、OpenAI/Claude/Gemini 的模型元数据与令牌计数 body 类型，
以及文档定义的原生媒体、工具和配置结构。内容生成模型、body 编解码与跨格式转换仍在开发。

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
