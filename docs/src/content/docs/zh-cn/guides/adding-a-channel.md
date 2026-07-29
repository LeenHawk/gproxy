---
title: 添加 Channel
description: 实现内置 adapter 或 native 编译时外部 Channel crate，并将其链接进自定义 GPROXY 二进制。
---

Channel 是上游访问适配器。它负责注入认证、解析 endpoint URL、分类上游响应，并可选处理
provider-specific request、response、stream、login、refresh 和 usage 行为。它不负责跨协议
transform，也不负责 provider rule-set processing。

```text
transform/     按 Operation 做协议转换
process/       transform 之后执行 provider rule set
channel/       上游访问、认证、endpoint、response disposition
```

:::caution
外部 Channel 是在编译时链接进自定义 **native** GPROXY binary 的可信 Rust 代码。它不是运行时
加载的 plugin、共享库、sandbox 或热重载机制。官方 GPROXY binary 只包含官方构建时编译的
Channel。
:::

## 选择集成方式

| 方式 | 适用场景 | 注册方式 | Target |
| --- | --- | --- | --- |
| 外部 crate | 不修改 GPROXY 源码的私有或仓库外 adapter。 | native 进程启动时收集 `linkme` constructor。 | 仅 native。 |
| 内置 Channel | 上游贡献、官方分发或 edge 支持。 | root registry 显式条目和 Cargo channel feature。 | native；兼容时也支持 edge。 |

两条路径实现相同的 `gproxy-channel-api::Channel` contract，并经过相同 routing 和执行
pipeline。外部路径需要一个很小的自定义 runner，因为新增 crate 会改变最终 executable。

## 从相似 Channel 开始

`src/channel/bulletins/` 中的内置 Channel 是可参考的实现：

| 上游形状 | 起点 |
| --- | --- |
| OpenAI-compatible API key | `openai`、`custom`、`deepseek`、`groq`、`nvidia` |
| Anthropic Messages | `claudeapi` |
| Gemini API key | `aistudio`、`vertexexpress` |
| Vertex service account | `vertex` |
| OAuth 或 agent envelope | `codex`、`claudecode`、`geminicli`、`antigravity`、`kiro`、`copilotcli` |

外部 crate 应复用这些概念，而不是 import root-private module。依赖
`gproxy-channel-api`，并使用它 re-export 的 `protocol` 和 `transform`，保证 Channel 与
host 使用完全相同的公开类型。

仓库中已编译验证的示例位于 `examples/external-channel/`：

```text
external-channel/
|-- channel/     # MIT adapter，仅依赖 gproxy-channel-api
`-- app/         # AGPL 自定义 runner，链接 gproxy 和 adapter
```

## 创建外部 Crate

adapter 需要公开 API，以及供注册 attribute 使用的直接 `linkme` 依赖：

```toml
[package]
name = "my-gproxy-channel"
version = "0.1.0"
edition = "2024"

[dependencies]
gproxy-channel-api = { version = "2", features = ["external-channels"] }
http = "1"
linkme = "0.3"
```

API release 必须与自定义 runner 使用的 GPROXY source 或 tag 匹配。Cargo package identity
不仅包含名称和版本，也包含依赖来源。如果 adapter 从 crates.io 解析
`gproxy-channel-api`，host 却解析到另一份 Git/path copy，程序可能正常编译，却注册到了另一份
linker slice。开发时让两者使用同一个 checkout；仓库外 workspace 则把 crates.io patch 到
同一个 GPROXY tag：

```toml
[patch.crates-io]
gproxy-channel-api = {
  git = "https://github.com/LeenHawk/gproxy",
  tag = "vX.Y.Z",
}
```

确认 runner 里只有一个 package identity：

```bash
cargo tree -p my-gproxy-runner -i gproxy-channel-api
```

## 实现 `Channel`

必须实现四个方法：

| 方法 | 责任 |
| --- | --- |
| `id()` | 稳定 registry id；它会成为 `Provider.channel`。 |
| `provider_family()` | 已有的 `OpenAi`、`Claude` 或 `Gemini` 协议 family。 |
| `routing_table()` | 声明 `(Operation, OperationKind) -> RoutingDecision` 能力表。 |
| `prepare()` | 构建绝对上游请求并注入认证。 |

常用可选 hook：

| Hook | 何时使用 |
| --- | --- |
| `metadata()` | Admin API 和通用 Console 表单需要名称、settings、credential、login mode、endpoint 或 usage 能力。 |
| `classify()` | 上游 status/header 需要 provider-specific retry、cooldown 或 auth-dead 处理。 |
| `shape_request()` | transform/process rule 后的 provider-native body 还需要字段清理。 |
| `shape_response()` | 原始上游 body 在 response transform 前需要归一化。 |
| `stream_decoder()` | envelope 或 binary stream 要在 SSE transform 前解码。 |
| `needs_refresh()` / `refresh()` | OAuth-like credential 需要使用前刷新。 |
| `prepare_usage_request()` / `parse_usage()` | provider 暴露 per-credential usage/quota endpoint。 |

`prepare()` 收到协议 transform 和配置化 process rule 之后的 body。把 `ctx.body` 移进请求，
并使用 absolute URI。不要复制全部下游 header：只转发 allow-list，并显式注入上游 credential。
provider-native body 清理应放在 `shape_request()`。

host 独占 persistence、secret encryption、refresh lease、proxy selection、TLS/HTTP client
resolution 和 request capture。异步 refresh、login 和自定义多步 exchange 会收到解析完成的
`UpstreamClient` 能力，因此 Channel 可以执行任意 HTTP、WebSocket 或 stream 调用，但不自行
创建绕过宿主管理的 client。

完整 API-key adapter 见 `examples/external-channel/channel/src/lib.rs`。它解析 settings、注入
bearer auth、只保留允许的 header、声明 routes，并提供 runtime metadata。

## 声明 Operation Routing

使用公开 route helper 与 protocol re-export：

```rust
use gproxy_channel_api::protocol::{
    ContentGenerationKind::*, Operation::*, Provider as P,
};
use gproxy_channel_api::routes::{cg, pass, pv, xform};

vec![
    pass(ListModels, pv(P::OpenAi)),
    xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
    pass(GenerateContent, cg(OpenAiChatCompletions)),
    xform(
        GenerateContent,
        cg(ClaudeMessages),
        GenerateContent,
        cg(OpenAiChatCompletions),
    ),
]
```

Routing 必须 Operation-first。每个 cell 单独说明这个 Channel 对 operation 与 wire kind 是
passthrough、transform、local handling，还是显式 unsupported。创建 Provider 时会把 route list
写成 `routing_rules`；运行时缺少 row 就是 unsupported。

外部 crate 可以使用已有 provider family、operation 和 transform topology。新增 core enum
variant 或第四种协议 family 仍需要协调内置修改，因为这些 enum 属于共享 API。

## 添加 Console Metadata

`Channel::metadata()` 驱动需要鉴权的 `GET /admin/channels`，以及 Console 的通用 Provider 与
Credential 表单。外部目录项标记为 `source: "external"`；Console 的静态 overlay 仅用于内置
Channel。

| `ChannelMetadata` 字段 | 用途 |
| --- | --- |
| `id`, `display_name` | registry identity 和面向管理员的名称。 |
| `provider_family` | OpenAI、Claude 或 Gemini 协议 family。 |
| `credential_family` | API key、OAuth tokens、service account 或 GitHub token。 |
| `login_modes` | `ChannelLogin` 提供的 auth-code、device-code 或 cookie flow。 |
| `settings_fields` | Console 渲染的通用 Provider settings。 |
| `secret_template` | 新建 Credential 时使用的 JSON 形状。 |
| `endpoint_kinds` | adapter 能识别的精确 endpoint override 字段。 |
| `usage` | 是否支持 credential 实时 usage。 |

setting control 支持 `text`、`url`、`boolean`、`integer` 和 `string_list`。
`ChannelMetadata::new(id, family)` 提供最小 API-key 默认值；配置更复杂时覆盖对应字段。外部
Channel 不应修改 Console 的静态内置 `CHANNELS` 表。

如果 Channel 支持交互式登录，实现 `ChannelLogin`。所有 login context 都包含 Provider
settings；登录成功的 secret 仍由 host 走统一 encryption 与 persistence 路径。

## 注册 Channel

### 编译时注册外部 Crate

把 native constructor 导出到 API crate 的 distributed slice：

```rust
use std::sync::Arc;
use gproxy_channel_api::{ChannelRegistration, RegisteredChannel};

#[cfg(not(target_arch = "wasm32"))]
fn register() -> RegisteredChannel {
    RegisteredChannel::new(Arc::new(MyChannel))
}

#[cfg(not(target_arch = "wasm32"))]
#[linkme::distributed_slice(
    gproxy_channel_api::registration::CHANNEL_REGISTRATIONS
)]
static REGISTER: ChannelRegistration = register;
```

同一个 id 还实现 `ChannelLogin` 时，使用 `RegisteredChannel::with_login(...)`。注册只发生在
启动期；Channel 或 login id 重复会让启动失败，不会覆盖内置项，也不会依赖 linker order。

### 注册内置 Channel

内置贡献需要在 `src/channel/bulletins/mod.rs` 中加入 module，把对应 `channel-*` Cargo feature
加入正确的 native/edge umbrella，并把实现加入 `src/channel/registry.rs` 的
`builtin_channels()`。有交互式登录时加入 `builtin_logins()`；需要时再分别把 metadata 加到
`src/channel/metadata.rs`，把 host-owned emulation profile 加到
`src/channel/emulation.rs`。

## 构建自定义 Native 二进制

最终 binary 必须显式引用 Channel crate，让 linker 保留其 registration section：

```rust
use my_gproxy_channel as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gproxy::native::run_cli().await
}
```

`gproxy::native::run_cli()` 保留标准 CLI、migration、bootstrap、storage、Console server 和
native channel collection。自定义 runner 链接 AGPL `gproxy` 应用；它的许可与源码分发义务
要和只依赖 MIT Channel API 的 adapter crate 分开考虑。

仓库示例会编译并测试完整组合：

```bash
cargo test \
  --manifest-path examples/external-channel/Cargo.toml \
  -p gproxy-example-channel-bin \
  --test linked_registration

cargo build \
  --manifest-path examples/external-channel/Cargo.toml \
  -p gproxy-example-channel-bin
```

仓库外 runner 应从一个固定 Git tag 或 checkout 依赖 root `gproxy` package，并让 API patch 使用
同一来源。升级 GPROXY 时重新构建 runner。通用 native build 和 Console asset 说明见
[Release 构建](/zh-cn/deployment/release-build/)。

## 验证已编译的 Channel

1. 使用标准 GPROXY 配置启动自定义 runner。
2. 以管理员身份认证并请求 `GET /admin/channels`。
3. 确认目标 id 存在，且 `source: "external"`。
4. 确认 Console Provider selector 和通用字段符合 `metadata()`。
5. 创建 Provider，并检查 `routing_table()` 生成的 routing rules。
6. 加入生产 Route 前，先发送一次 Provider-scoped 请求。

## 限制与升级

- 编译时外部注册仅支持 native。edge build 使用显式内置 registry。
- 外部代码与 GPROXY 同进程、同权限运行，没有 ABI isolation 或 sandbox。
- 不重新构建并重启 binary，就不能加载、卸载或升级 Channel。
- 官方 binary、container 和 edge bundle 不会自动获得仓库外 adapter；应分发自定义 runner。
- 对自定义 runner 应用 GPROXY 官方 self-update，可能会把它替换为不含外部 crate 的官方
  binary。升级时应重新构建并部署自定义 runner。
- Channel 未出现时，先检查 `use my_gproxy_channel as _;`，再用 `cargo tree` 确认只有一份
  `gproxy-channel-api` package source。
