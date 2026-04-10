---
title: 开发参考
description: 构建命令、工作区结构、Channel 贡献指南和数据目录。
---

## 命令

### 后端

```bash
cargo fmt --all --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments
cargo test --workspace --all-targets --no-fail-fast
cargo run -p gproxy
```

CI 执行的就是这些命令。`clippy` 将所有警告视为错误（`-D warnings`），但 `too_many_arguments` 除外。

### 前端

```bash
cd frontend/console
pnpm install
pnpm typecheck
pnpm test
pnpm build
```

构建后的控制台通过 `rust-embed` 嵌入到 Rust 二进制中，路径为 `apps/gproxy/web/console`。在 `cargo build --release` 之前先运行 `pnpm build` 以包含最新前端。

### 文档

```bash
cd docs
pnpm install
pnpm build
```

文档使用 Starlight（基于 Astro）。开发服务器：`pnpm dev`。

## 工作区结构

| 路径 | 包名 | 说明 |
|---|---|---|
| `sdk/gproxy-sdk` | `gproxy-sdk` | 聚合导出 provider + protocol + routing SDK 的 re-export crate |
| `sdk/gproxy-protocol` | `gproxy-protocol` | 协议定义（operation 族、protocol 类型、transform） |
| `sdk/gproxy-provider` | `gproxy-provider` | Channel trait、dispatch 表、所有 channel 实现 |
| `sdk/gproxy-routing` | `gproxy-routing` | 模型路由、provider 前缀处理、model 别名解析 |
| `crates/gproxy-core` | `gproxy-core` | 共享核心类型与工具 |
| `crates/gproxy-storage` | `gproxy-storage` | 数据库层（SeaORM，支持 SQLite/MySQL/PostgreSQL） |
| `crates/gproxy-api` | `gproxy-api` | HTTP API 层（Axum 路由、鉴权、admin/user/provider 处理器） |
| `crates/gproxy-server` | `gproxy-server` | 服务运行时（AppState、中间件、内存缓存、session） |
| `apps/gproxy` | `gproxy` | 主程序（CLI、配置、Web 服务、启动流程） |
| `apps/gproxy-recorder` | `gproxy-recorder` | MITM 录制代理，用于捕获 provider 流量 |
| `frontend/console` | `@gproxy/console` | 管理控制台 SPA（SolidJS + TypeScript） |
| `docs` | - | 文档站点（Starlight） |

## 贡献 Channel

v1 使用基于 trait 的 Channel 架构，通过 `inventory` crate 自动注册。无需手动枚举关联。

### 步骤

1. 创建 `sdk/gproxy-provider/src/channels/your_channel.rs`（较大的 channel 可使用目录加 `mod.rs`）。

2. 实现 `Channel` trait：

```rust
use gproxy_provider::channel::{Channel, ChannelSettings, ChannelCredential};
use gproxy_provider::dispatch::DispatchTable;
use gproxy_provider::health::CredentialHealth;
use gproxy_provider::request::PreparedRequest;
use gproxy_provider::response::{ResponseClassification, UpstreamError};

pub struct YourChannel;

impl Channel for YourChannel {
    const ID: &'static str = "your_channel";

    type Settings = YourSettings;
    type Credential = YourCredential;
    type Health = YourHealth;

    fn dispatch_table(&self) -> DispatchTable { /* ... */ }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> { /* ... */ }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        body: &[u8],
    ) -> ResponseClassification { /* ... */ }
}
```

3. 实现关联类型：
   - `Settings` -- 实现 `ChannelSettings`（base URL、user agent、重试配置）
   - `Credential` -- 实现 `ChannelCredential`（API key、OAuth token）
   - `Health` -- 实现 `CredentialHealth`（alive/dead/cooldown 追踪）

4. 实现 `dispatch_table()`，返回 (operation, protocol) 到 channel 行为的路由映射。

5. 实现 `prepare_request()`，构建上游 HTTP 请求（设置 URL、鉴权头、请求体）。

6. 实现 `classify_response()`，根据上游响应决定重试策略（成功、限流、鉴权失败、服务端错误）。

7. 在模块作用域注册到 inventory：

```rust
inventory::submit! {
    ChannelRegistration::new(YourChannel::ID, your_dispatch_table)
}
```

8. 在 `sdk/gproxy-provider/src/channels/mod.rs` 中添加 `mod your_channel;`。

完成。无需手动枚举注册、无需修改 provider.rs、无需改 settings.rs。`inventory` 在链接时自动完成发现。

### 可选 trait 方法

`Channel` trait 提供了多个带默认实现的可选方法：

- `finalize_request()` -- credential 选择前的请求体归一化
- `normalize_response()` -- 修复上游非标准响应字段
- `count_strategy()` -- 覆盖 token 计数策略（默认使用本地 tiktoken）
- `handle_local()` -- 不访问上游直接本地处理请求
- `needs_spoof_client()` -- 使用浏览器伪装 HTTP 客户端
- `ws_extra_headers()` -- WebSocket 握手附加请求头
- `model_suffix_groups()` -- channel 特定的模型后缀
- `refresh_credential()` -- 鉴权失败后刷新凭证
- `prepare_quota_request()` -- 构建上游配额查询请求
- `oauth_start()` / `oauth_finish()` -- OAuth 流程支持
- `model_pricing()` -- 默认定价表

### 前端集成

如果 channel 需要自定义管理 UI（设置表单、credential 表单）：

1. 在 `frontend/console/src/modules/admin/providers/channels/your_channel/` 下添加 channel 文件。
2. 在前端 channel 注册表中注册。

### 验证

```bash
cargo check
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments
cargo test --workspace --all-targets --no-fail-fast
```

## 数据目录

| 路径 | 说明 |
|---|---|
| `./data` | 默认数据目录 |
| `./data/gproxy.db` | 默认 SQLite 数据库（`sqlite://./data/gproxy.db?mode=rwc`） |
| `./data/tokenizers` | HuggingFace tokenizer 缓存（首次使用时下载） |

`dsn` 配置项可切换到 MySQL 或 PostgreSQL。SQLite 为单实例部署的默认选项。
