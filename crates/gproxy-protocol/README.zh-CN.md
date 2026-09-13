# gproxy-protocol

[English](https://github.com/LeenHawk/gproxy/blob/4.0/crates/gproxy-protocol/README.md) | 简体中文

面向 OpenAI、Anthropic Claude 和 Google Gemini API 的 Rust 协议类型库。
可用于网关、SDK 和协议适配器，描述 API 操作及其数据格式。

> 本文介绍 v4 开发版 API。目前提供操作和方言标识；请求/响应模型与格式转换功能仍在开发中。

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

## 使用

`OperationKey` 将操作与表达该操作的格式组合在一起：

```rust
use gproxy_protocol::{Dialect, Operation, OperationKey};

let messages = OperationKey {
    operation: Operation::GenerateContent,
    dialect: Dialect::Claude,
};

let responses = OperationKey {
    operation: Operation::GenerateContent,
    dialect: Dialect::OpenAi,
};

assert_eq!(messages.operation, responses.operation);
assert_ne!(messages.dialect, responses.dialect);
```

键实现了 `Copy`、`Eq`、`Hash` 和 `Ord`，可直接用于映射表和集合。
构造键只是标识一种组合，不检查应用是否支持它。

| 方言 | 内容生成 API |
|---|---|
| `OpenAi` | OpenAI Responses |
| `Claude` | Anthropic Messages |
| `Gemini` | Gemini GenerateContent |
| `OpenAiChat` | OpenAI Chat Completions |
| `OpenAiResponsesWebSocket` | 通过 WebSocket 使用 OpenAI Responses |

`Operation` 还涵盖模型列表、token 计数、嵌入、图像、音频、视频、文件和实时会话。
流式内容生成使用 `StreamGenerateContent`。方言表示数据格式，与实际托管模型的服务无关。

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
