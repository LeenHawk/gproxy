# gproxy-protocol

[English](https://github.com/LeenHawk/gproxy/blob/4.0/crates/gproxy-protocol/README.md) | 简体中文

面向 AI API 网关、SDK 和协议适配器的 Rust HTTP/WebSocket 类型与宿主能力接口库。
将 HTTP 元信息与原始或强类型 body 放在同一条消息中，独立建模已建立的 WebSocket 连接。

当前为 v4 开发版，提供模型发现、令牌计数、内容生成与流式、上下文管理、嵌入、重排、
文件、图像、音频、视频和实时会话的原生类型。HTTP 元信息与强类型 body 分离；
WebSocket 消息和双向连接分别建模。独立能力 trait 描述宿主提供的上游调用、资源访问和作用域状态。

已声明字段与变体依据对应的厂商 API 或官方客户端结构。`rest` 保留未知扩展；只有来源
明确允许任意 JSON 的位置才保留任意 JSON 类型。已提供有界 body codec 和转换身份辅助；
网络客户端和语义转换属于后续实现阶段。

这些类型是对着厂商公开的文档写的；读取该文档、据此校验类型的测试与文档放在一起，
位于另一个仓库，不在此处。

## 安装

v4 预发布版本对应的 Cargo 依赖为：

```toml
[dependencies]
gproxy-protocol = "4.0.0"
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
use gproxy_protocol::wire::openai::models::{GetModelRequest, GetModelResponse, Model};

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

## Body codec

`codec` 提供有界 JSON、SSE、JSON 数组、NDJSON 和 multipart 编解码。
调用方显式提供 `CodecLimits`；transport chunk 可以切在 UTF-8、JSON token、MIME header
或 boundary 中间。解码器拒绝非法／截断输入，并执行累计 body 限额。

| Codec | 增量接口 |
|---|---|
| JSON 数组／NDJSON | `push` 返回已完整解析的值，最后调用 `finish` |
| SSE | `SseDecoder::push` 返回数据事件或 `[DONE]`；仅控制字段的块更新 ID／retry 状态 |
| Multipart | `MultipartDecoder::next_part` 返回 headers 和真正的流式 body |

请求下一 part 前，应将当前 body 读到 EOF 或丢弃。返回的 body 也会驱动原输入流，兼容
wasm 的非 `Send` 输入。`MultipartEncoder` 可接收 `Multipart` part 流，无需收集文件内容。
SSE 严格检查 EOF：未以空行终结的数据事件返回错误，不当作完整事件。
这些 codec 只解释分帧和 JSON 语法，厂商语义由 typed 转换处理。

## 内容生成的强类型载荷

厂商类型统一位于 `wire::{claude, gemini, openai}`。根级 `claude`、`gemini`、`openai`
保留同一套类型的重新导出，兼容已有引用路径。

| API | 请求与非流式响应 | 流式载荷 |
|---|---|---|
| Claude Messages | `wire::claude::generate_content` | `wire::claude::stream::StreamEvent` |
| Gemini | `wire::gemini::generate_content` | `wire::gemini::stream::StreamChunk` |
| OpenAI Responses | `wire::openai::responses::{generate, response}` | `wire::openai::responses::stream::StreamEvent` |
| OpenAI Chat Completions | `wire::openai::chat::{request, response}` | `wire::openai::chat::stream::ChatCompletionChunk` |

流式响应别名承载 `ByteStream`。调用方处理 SSE 或 JSON 数组分帧，再把每条 JSON 载荷
反序列化为对应类型。这些类型不负责拆解网络 chunk、累积模型输出或跨 API 转换；
Chat Completions 的 `[DONE]` 等非 JSON 传输标记也由调用方的分帧逻辑处理。

## 其他 API 模型

| 范围 | 模块 |
|---|---|
| 上下文与会话 | `wire::openai::{compact, conversation, memory}` |
| Codex 客户端接口 | `wire::openai::{guardian, web_search}` |
| 嵌入与重排 | `wire::openai::{embeddings, rerank}`、`wire::gemini::embeddings` |
| 文件操作 | `wire::openai::files`、`wire::claude::files`、`wire::gemini::files` |
| 图像与音频 | `wire::openai::{images, audio}` |
| 视频 | `wire::openai::video`，扩展格式与原生格式独立 |
| Responses WebSocket | `wire::openai::responses::websocket` |
| 实时 / WebRTC | `wire::openai::realtime`、`wire::gemini::live` |
| Gemini 声音管理 | `wire::gemini::voices`（snake_case 创建、列表、查询、删除的报文与查询参数） |

2026-10-01 的 Gemini 文档核对补充了自定义声音、语音样式、视频动态处理、
转写配置及结果、Computer Use 配置和最新安全类别及结束原因。共享音频类型位于
`wire::gemini::audio`，原有 Live 导出路径保持可用。Voices 类型建模不包含网关路由；
Interactions 专属的 Omni、音乐和托管 Agent 操作不属于现有 GenerateContent 方言。

multipart 表单包含明确的元数据字段和真实的 `MultipartPart` 文件流，下载响应保留原始 body。
类型本身不上传文件、不抓取结果 URL，也不在原生与扩展视频格式之间执行转换。

`spec::OPERATION_SPECS` 描述已建模的操作／方言组合及 wire 格式，不代表某个供应商支持该操作，
也不代表转换已经存在。TLS、ALPN 和 HTTP/2 客户端配置归出站 client 实现。

## 宿主能力

`capability` 定义三个独立 trait，供宿主为协议适配提供上游调用、资源访问或作用域状态。
适配可以组合多次调用，无需依赖完整网关 core。

| Trait | 操作 |
|---|---|
| `Upstream` | 发送已编码的 HTTP 请求；建立 WS 并保留握手响应 |
| `ResourceAccess` | 解析／读取资源；发布、查询发布结果、释放有所有权的发布资源 |
| `StateStore` | 读取作用域状态；以 CAS 原子创建、替换或删除 |

`ResourceReference` 区分上游 ID 和 URL。发布指定所需形态和未来到期时间；宿主不支持时
在发布前拒绝。释放或到期后，操作 ID 保留为过期状态，重试不会重新创建资源。

宿主提供目标、鉴权、所有权检查和绑定的 `CapabilityLimits`，并在整个传输期间执行时限
和字节限制，返回 stream 后也不例外。`CapabilityFuture` 在原生平台要求 `Send`，wasm
允许非 `Send`，不依赖特定异步 runtime、HTTP client 或数据库。

非成功 HTTP 状态仍以 `WireResponse` 返回；传输／存储错误使用 `CapabilityError` 并保留
原始错误来源。WS 握手拒绝保留完整 HTTP 响应 body。取消只停止本地处理，不保证远端回滚；
发布可以按作用域内的操作 ID 查询结果。

这些 trait 是宿主必须遵守的契约，尚未附带能力实现、语义转换器或网关 core。
后续转换只使用明确声明的 wire 字段：不读取源 `rest`，也不填充目标 `rest`。

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

[MIT](LICENSE)。

## 兼容服务的模型元数据

OpenAI 模型对象沿用 v3 固定的 GPROXY 模型元数据扩展；兼容服务未提供创建时间或归属时，
对应字段可以缺省。协议层与 Codex 渠道均不根据 UA 切换模型目录的输出格式。

## 兼容服务的推理内容

Chat 的 assistant 历史消息、响应及流式 delta 显式声明 DeepSeek 的 `reasoning_content`，
以及 OpenRouter 的 `reasoning` / `reasoning_details`（文本、摘要和加密内容变体）。
同协议序列化与反序列化保留各字段名称、空值/null 和结构化明细。流聚合按 index 或 ID
拼接明细片段，拆流时输出聚合后的明细。跨方言转换可见推理时，优先选择一个文本字段，
缺失时再使用明细中的文本/摘要，避免服务同时返回两种表达时重复输出。
Claude thinking、Gemini thought 文本及 Responses reasoning 映射为 `reasoning_content`；
Chat 明文推理映射为这些协议对应的响应/流式内容类型。
Responses 的 reasoning ID、摘要/文本片段与密文通过 `format=openai-responses-v1` 的
reasoning_details 携带；Claude thinking 的文本/签名和 redacted 块使用
`format=anthropic-claude-v1`。请求和响应转换只还原匹配格式，不把其他格式的密文当成本厂商数据。
format 是协议判别字段，不是密码学验证；调用方仍须将回放路由到原来的厂商和模型。
流式转换在块/响应完成后发送完整明细；Chat 转回原生格式时，在 choice 完成后输出 reasoning，
避免晚到的 ID/签名改变已经输出的条目。普通回答文本仍然增量输出。
文本/摘要片段追加，加密数据和签名按快照处理，重复快照不会拼接损坏密文。
没有匹配签名明细的 Chat 明文推理仍使用空 Claude 签名。Responses 与 Claude 直接互转继续使用
已有的原始来源绑定状态还原路径。
