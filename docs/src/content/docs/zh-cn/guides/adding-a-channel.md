---
title: 新增通道 (Adding a Channel)
description: 如何在 GPROXY 中实现一个新的上游通道 —— Channel trait、routing table、注册以及 Cargo feature。
---

**通道 (channel)** 是一段说特定上游 wire 协议的代码。新增一个上游 =
实现一次 `Channel` trait、声明默认 routing table、用 `inventory`
注册 —— SDK 和 server 就都能自动发现它。

本页只给最少步骤。请把这一页和现有通道
[`sdk/gproxy-provider/src/channels/`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/channels/)
一起看 —— 现有通道是权威参考，新通道通常从"复制最接近的那个"开始。

## 组成部分

一个通道由 5 个部分构成：

1. **Settings 结构体** (`ChannelSettings`) —— `base_url`、超时、缓存规则、
   rewrite 规则、以及通道专有的开关。按供应商保存。
2. **Credential 结构体** (`ChannelCredential`) —— API key、OAuth token、
   service account、cookie 之类。同一个供应商可以挂多张凭证，引擎会轮转。
3. **Health 类型** (`CredentialHealth`) —— 通常是 `ModelCooldownHealth`，
   按 `(credential, model)` 粒度在可重试失败时冷却凭证。
4. **`Channel` impl** —— 这个 trait 包括 `prepare_request`、
   `classify_response`、可选的 `finalize_request` / `normalize_response` /
   `handle_local`，以及默认 `routing_table`。
5. **注册** —— 一段 `inventory::submit!` 块，给注册表添加
   `ChannelRegistration`，启动时被自动发现。

trait 定义在
[`sdk/gproxy-provider/src/channel.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/channel.rs)。

## 选起点

动手之前，先复制一个和目标最接近的现有通道：

| 你的上游是… | 从哪个开始 |
| --- | --- |
| OpenAI 兼容（自建、第三方网关） | `openai.rs` 或专门的 `custom.rs` |
| Anthropic 系（Messages API、`cache_control`） | `anthropic.rs` |
| Gemini / Vertex | `aistudio.rs` / `vertex.rs` / `vertexexpress.rs` |
| OAuth / cookie 鉴权的开发工具 | `claudecode.rs` / `codex.rs` |

改掉 struct、module 和 `ID` 常量之后再逐步往外做改造。

## 1. 定义 settings 和 credential

```rust
// sdk/gproxy-provider/src/channels/acme.rs
use serde::{Deserialize, Serialize};

use crate::channel::{Channel, ChannelCredential, ChannelSettings};
use crate::routing::{RoutingTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

pub struct AcmeChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcmeSettings {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries_on_429: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sanitize_rules: Vec<crate::utils::sanitize::SanitizeRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewrite_rules: Vec<crate::utils::rewrite::RewriteRule>,
}

fn default_base_url() -> String { "https://api.acme.example/v1".into() }

impl ChannelSettings for AcmeSettings {
    fn base_url(&self) -> &str { &self.base_url }
    fn user_agent(&self) -> Option<&str> { self.user_agent.as_deref() }
    fn max_retries_on_429(&self) -> u32 { self.max_retries_on_429.unwrap_or(3) }
    fn sanitize_rules(&self) -> &[crate::utils::sanitize::SanitizeRule] { &self.sanitize_rules }
    fn rewrite_rules(&self) -> &[crate::utils::rewrite::RewriteRule] { &self.rewrite_rules }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcmeCredential {
    pub api_key: String,
}

impl ChannelCredential for AcmeCredential {}
```

在 settings 上暴露 `sanitize_rules` 和 `rewrite_rules` 是把你的通道
插入那两条跨切关注点（[请求改写规则](/zh-cn/guides/rewrite-rules/)和
脱敏器）的方式 —— 只要三行，几乎没有理由省掉。

## 2. 实现 `Channel`

必选方法只有两个：`routing_table` 和 `prepare_request`。其它都有默认值。

```rust
impl Channel for AcmeChannel {
    const ID: &'static str = "acme";
    type Settings   = AcmeSettings;
    type Credential = AcmeCredential;
    type Health     = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        let mut table = RoutingTable::new();
        let pass = |op, proto| (
            RouteKey::new(op, proto),
            RouteImplementation::Passthrough,
        );

        // Acme 原生就说 OpenAI Chat Completions。
        for (key, imp) in [
            pass(OperationFamily::ModelList,              ProtocolKind::OpenAi),
            pass(OperationFamily::GenerateContent,        ProtocolKind::OpenAiChatCompletion),
            pass(OperationFamily::StreamGenerateContent,  ProtocolKind::OpenAiChatCompletion),
        ] {
            table.set(key, imp);
        }
        table
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings:   &Self::Settings,
        request:    &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let url = format!("{}{}", settings.base_url, request.upstream_path);
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(url)
            .header("Authorization", format!("Bearer {}", credential.api_key));

        for (k, v) in request.headers.iter() {
            builder = builder.header(k, v);
        }
        builder
            .body(request.body.clone())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn classify_response(
        &self,
        status: u16,
        _headers: &http::HeaderMap,
        _body:    &[u8],
    ) -> ResponseClassification {
        match status {
            429 | 500..=599 => ResponseClassification::RetryableWithCooldown,
            401 | 403       => ResponseClassification::DisableCredential,
            _               => ResponseClassification::Pass,
        }
    }
}
```

可选的 hook 你大概率会用到：

- **`finalize_request`** —— Anthropic / ClaudeCode 就在这儿应用
  [cache 断点](/zh-cn/guides/claude-caching/)和魔法字符串触发器。
  如果你的上游需要"对路由或缓存亲和逻辑也可见"的 body 规范化，放这里 ——
  不要放 `prepare_request`。
- **`normalize_response`** —— 在 usage 抽取 / 协议翻译前，修正非标准的
  响应字段。
- **`handle_local`** —— 实现 routing 表中的 `Local` 路由。大多数通道
  只在搭配 `*-only` 预设时给 `model_list` / `model_get` 用。
- **`model_pricing`** —— 返回 `&'static [ModelPrice]`，让 GPROXY 无需管理员
  手填价格就能记账。参考 `channels/pricing/` 下现有的 JSON 模式。

## 3. 注册通道

注册就是文件末尾的一个 `inventory::submit!` 调用。它必须放在 `impl` 块
之外 —— 这是注册表能在启动时发现该通道的关键：

```rust
fn acme_routing_table() -> RoutingTable {
    AcmeChannel.routing_table()
}

inventory::submit! {
    ChannelRegistration::new(AcmeChannel::ID, acme_routing_table)
}
```

`ChannelRegistry::collect()` 在启动时迭代 `inventory`，按 `ID` 索引每个
已注册的通道。不存在任何要手动维护的 `match` 表。

## 4. 把模块接到 crate 里

在
[`sdk/gproxy-provider/src/channels/mod.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/channels/mod.rs)
里声明模块：

```rust
pub mod acme;
```

如果你想为它加一个 Cargo feature flag（允许用户把不用的通道从二进制里
剥出去），在
[`sdk/gproxy-provider/Cargo.toml`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/Cargo.toml)
和
[`sdk/gproxy-sdk/Cargo.toml`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-sdk/Cargo.toml)
两处都加上：

```toml
# gproxy-provider
[features]
acme = []
all-channels = ["openai", "anthropic", /* … */, "acme"]
```

注意：现有 feature flag 只是声明了通道名，目前源码里并没有任何
`#[cfg(feature = "...")]` 把代码包起来 —— 所有通道都会被一起编译。如果
你真的希望 feature off 时把通道排除出去，把模块导入和 `inventory::submit!`
都用 `#[cfg(feature = "acme")]` 包住。

## 5.（可选）添加 TypeScript UI 编辑器

内嵌控制台会为通道渲染通道感知的结构化 settings / credentials 编辑器。
想让新通道拥有这种体验而不是通用 JSON 文本框，就在
`frontend/console/src/modules/providers/channels/` 下按你的 settings 形状
加一份 schema 定义。现有通道就是模板 —— 做法是一份 TS 类型 + 一份 form
定义。

## 测试

开 PR 前的几个检查：

- **`cargo test -p gproxy-provider`** —— 通道测试就放在通道实现旁边。
- **`cargo run -p gproxy`** 配一份 `channel = "acme"` 并在 `credentials`
  里带 `AcmeCredential` 的种子 TOML —— 验证注册表、settings 反序列化、
  routing 表全都对齐。
- **用真正的上游跑一遍 `curl`**，通过限定作用域路径（`/acme-test/v1/chat/completions`）—— 限定作用域是调试新通道最干净的隔离方式，详见
  [发送第一个请求](/zh-cn/getting-started/first-request/)。

## 自检清单

- [ ] `ChannelSettings` / `ChannelCredential` 结构体带 `serde` 默认值。
- [ ] `Channel` impl 有 `routing_table`、`prepare_request`、
  `classify_response`。
- [ ]（如需）`finalize_request`、`normalize_response`、`handle_local`。
- [ ] 文件末尾的 `inventory::submit!` 注册。
- [ ] `channels/mod.rs` 里的 `pub mod your_channel;`。
- [ ]（可选）`gproxy-provider/Cargo.toml` 和 `gproxy-sdk/Cargo.toml` 里
  的 Cargo feature。
- [ ]（可选）`channels/pricing/` 下的价格 JSON。
- [ ]（可选）`frontend/console` 下的结构化编辑器。
- [ ] 通过限定作用域路径对真实上游做冒烟测试。
