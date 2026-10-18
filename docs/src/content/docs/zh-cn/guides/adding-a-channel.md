---
title: 新增渠道
description: "一个 v4 渠道的结构：BaseChannel 契约、可选能力 trait、从一个 Cargo feature 到注册完成的步骤，以及如何在本仓库之外发布一个渠道。"
---

一个渠道只知道一族上游：它的 URL、凭证怎么注入、它讲哪些线方言、它的流怎么报 usage，
以及——对账号型上游——怎么登录、刷新和读配额。

其余都是别人的事。路由、凭证选择、失败转移、协议转换、结算和 capture 属于引擎，而且
**渠道里没有任何东西读数据库或挑传输后端**。

渠道是编译进去的，从不在运行时加载，它可以放在两个地方：

- **放在本仓库里**：`gproxy-channel` 的一个模块，藏在它自己的 Cargo feature 后面，通过 PR
  加入。它能到达每一个宿主，包括 Workers 宿主。见[九步](#九步)。
- **放在你自己的 crate 里**：一个对着公开契约实现 `BaseChannel` 的类型，加上一个"原版
  + 这个类型"的原生二进制或 Worker。本仓库里什么都不用改。见[在本仓库之外](#在本仓库之外)。

两种方式实现的是同一个 trait，写法也一样；不同的只是注册方式。默认不编译任何具体渠道。

## 它需要一个渠道吗

渠道是要对着别人的 wire 维护的代码。只有当一行 Provider 说不清它需要什么时——路径随操作
移动、body 必须被改写、有账号 surface 要读、有客户端身份要呈现——一个厂商才值得一个渠道。

全部差别只是一个源和一个 header 的厂商，就是一个 `custom` Provider。见
[不需要渠道的厂商](/zh-cn/guides/providers/#接入兼容服务)。

## 契约

`BaseChannel` 是唯一必需的 trait，`id` 是它唯一必需的方法。每个协议操作都有自己的异步方法
并带**默认实现**：用 `prepare`（HTTP）或 `prepare_connect`（WebSocket）构造请求，再把它交给
被指派的 client。

两个准备钩子的默认实现都是"不支持"，因此**一个渠道恰好支持它准备或覆写的那些，绝不更多**。

```text
宿主选定 Provider、凭证、client
  → ChannelBinding::new(&channel, provider, credential, client)
  → binding.send(OperationKey, WireRequest<HttpBody>)
    binding.connect(OperationKey, WireRequest<()>)
  → BaseChannel 上那个具名操作方法
  → 默认：prepare + client.send  /  prepare_connect + client.connect
    或渠道自己经同一个 client 的多次调用流程
```

宿主交过来的视图是借用的，能公开的都公开：

| 类型 | 装什么 |
| --- | --- |
| `ProviderView` | `id`、`channel`、可选 `base_url`，以及渠道自己解码成类型化设置的 `config` JSON |
| `CredentialView` | `id`、`provider_id`、`auth_kind`、`secret` JSON（无 `Debug`、无 `Serialize`）、宿主记录的公开 `metadata`、`version`、`expires_at_ms` |
| `OperationContext` | 两个视图、操作的方言、`WireRequest`、一个持有的 client、跨请求状态、宿主 `instance_id` 和可选的端点覆盖 |
| `CredentialContext` | 刷新、配额与服务用的 Provider、凭证和 client |

### 每个渠道都遵守的规则

- **准备是同步且纯的**：不做 I/O、不藏状态、不构造 client。操作覆写可以做多次交换，但只能
  经 `context.client`，并且可以在响应流结束后继续持有它完成收尾工作。
- **来源鉴权绝不到达上游。** 转发助手丢弃逐跳 header，外加 `host`、`content-length`、
  `authorization`、`x-api-key`、`x-goog-api-key` 和 `api-key`；渠道从凭证里加上自己的
  鉴权。全局白名单、Provider 的 `allowed_headers` 与渠道声明的头取并集，
  `content-type` 总是保留。未配置或空列表不增加允许项。
- **端点覆盖是完整的方法 URL**，替换 base URL 加渠道默认路径——不是一个拿来再拼的 base。
- **非 2xx 是一个响应，不是一个错误。** 状态、header 和一个惰性 body 原样回来。错误变体
  是给*能力*调用（登录、刷新、配额）和多次调用覆写里的中间交换用的，那里失败的应答不是
  要返回的那个响应。
- **`RefreshRejected` 意味着上游确定性地拒绝了这把凭证**（`invalid_grant`、已撤销），
  宿主会把它标记为死。短暂的传输失败或 5xx 绝不能：它必须以传输错误浮现，好让宿主稍后
  重试。这一条弄错就会杀掉一个正常的账号。
- **`ContinuationElsewhere`** 表示一条活的上游连接被另一个宿主进程持有。宿主会改道；
  凭证本身没有任何问题。

## 可选能力

每一项都是一个独立 trait，藏在默认返回 `None` 的访问器后面。渠道实现它的上游拥有的那些，
而且彼此之间没有任何强制耦合——一个渠道可以报告配额窗口却不提供重置。

| 访问器 | 用途 |
| --- | --- |
| `credential_refresh` | 产出一份**完整替换**的密钥与过期时间；宿主以 `version` 上的 CAS 写回 |
| `oauth_authorization_code` | 用宿主提供的 PKCE challenge 与 state 构造授权 URL；把 code 换成凭证 |
| `oauth_device_code` | `start` 与一步 `poll`。节奏是宿主的事 |
| `cookie_login` | 把粘贴来的 cookie 换成一份凭证加它的公开元数据 |
| `quota_model` | 同步且纯：从 `auth_kind` 与 metadata 推出这把凭证有哪些配额维度 |
| `quota_query` | 从上游的用量端点读一份配额快照 |
| `quota_headers` | 把响应 header 变成配额条目；空表示什么都没报 |
| `quota_reset` | 在卖重置额度的上游上兑换与手动重置 |
| `usage_extras` | 厂商自己的用量字段，在宿主已经从整形后的响应里读到的标准用量之外补充 |
| `response_reason_observer` | 给原始上游响应分类（限流、过载、拒绝……）。默认实现解析标准 JSON 与 SSE；只有私有 wire 格式才需要覆写 |
| `websocket_response_reason` | 同上，针对一条完整的入站 WebSocket 消息 |
| `services` | 厂商控制面路由——见 [CLI 客户端](/zh-cn/guides/cli-clients/) |

用量本身不需要任何能力：宿主从渠道整形出的标准响应里读取它，一个什么都没报的渠道是
**未报告**，不是 0。

有两项 descriptor 能力无法从访问器推出，具备它们的渠道要在自己的 `descriptor` 里写明：
`websocket`（它覆写了某个 socket 操作）和 `paid_usage`（它的
`QuotaModel::allows_paid_usage` 读取凭证 metadata 标志 `allow_paid_usage`，控制台正是据此
显示那个开关）。

跨请求记忆由宿主限定在一个 Provider 加一把凭证的范围内，并以 CAS 写入。binding 的默认实现
拒绝一切写入，因此想要状态的渠道必须被明确授予。

### 登录口袋

两种 OAuth 流程都带着同一个口袋，用来存渠道在用户完成授权之后需要的事实：`start` 放进去
的东西会随授权一起回来。它的形状是渠道自己的——一个非标准的设备句柄、一个登录过程为自己
注册的 client。

**这个口袋可以带密钥**，与宿主会当作凭证 metadata 公开的 `provider_fields` 不同：它活在
宿主的登录会话里，短命、由 cache 承载、从不被渲染，登录结束就没了。任何必须活过登录的
东西都要从 exchange 返回——公开事实进 metadata，秘密进 provider secrets，后者与令牌一起
密封，永远不会变成 metadata。

## 九步

拿 `custom`（API key）和 `codex`（OAuth 账号）当两个参考。

1. **在 `crates/gproxy-channel/Cargo.toml` 里声明 feature**，只列这个渠道需要的可选依赖。
   仅 wasm 需要的依赖放进 `cfg(target_arch = "wasm32")` 的 target 表。

   ```toml
   [features]
   # 一行说明上游是什么、这个渠道覆盖到哪。
   acme = ["dep:base64", "dep:web-time"]
   ```

2. **往 `src/channels/mod.rs` 的 `channels!` 列表里加一行**：feature、模块，以及宿主要注册
   的那个值。这一行同时声明模块并把渠道放进 `compiled_in()`，而那正是每个宿主找到它的方式。

   ```rust
   "acme" => acme, acme::Acme;
   ```

   如果这个渠道用到 `shared`，也要扩展那个模块自己的 `cfg(any(...))`。

3. **一个文件一件事地铺开模块。** 小渠道就是一个 `acme.rs`；大的是一个目录：

   ```text
   src/channels/acme/
     mod.rs        id、单元结构体、re-export、模块文档
     config.rs     Provider 的 `config` JSON，serde(default)，忽略未知键
     request.rs    prepare / prepare_connect / 被覆写的操作
     oauth.rs      登录与刷新能力
     quota.rs      配额能力
     usage.rs      用量能力
     services.rs   厂商控制面路由
   ```

   **模块文档要点名每一条 wire 事实的来源**——厂商的 API 参考，或 `samples/` 下的 CLI
   源码。wire 真相绝不是另一个渠道的代码。

4. **实现 `BaseChannel`。** 最少是 `id`、`native_dialects` 和 `prepare`。结构体是无状态
   单元，所以一个实例服务该渠道的每一个 Provider。

   也要覆写 `descriptor`。默认实现只读能力访问器，而只有渠道自己知道它的显示名，以及它
   从 Provider `config` JSON 里解码哪些键。**那份 descriptor 是管理 UI 渲染 Provider 表单
   的全部依据**——在 `ts` feature 下它还是一个 TypeScript 类型——所以漏掉的键就是没人能设的键。

   当上游需要多次交换、需要本地合成的应答、需要另一种传输，或需要把响应改写成声明的原生
   方言时，覆写具体的操作方法而不是 `prepare`。

5. **声明厂商客户端会发什么。** 如果渠道模拟某个 CLI，导出它的 header 常量并用它构造
   allow-list，这样 Provider 的 `allowed_headers` 就剥不掉 CLI 自己的 header。把这些身份
   header 传进转发助手的丢弃列表，让**客户端无法伪造它们**。上游对客户端做指纹时返回一份
   默认连接配置；凭证或 Provider 上的显式配置依然胜出。

6. **把能力做成独立类型**并从访问器返回。守住每个 trait 的规则：刷新返回完整替换而不是
   合并；配额维度从凭证 metadata 读套餐事实，绝不走网络；用量观察者做累积快照，而它的
   `finish` 从**宿主**收到"完成还是被打断"，因为光有 EOF 并不能确立完整的用量。

7. **可复用的 wire 机制放进 `src/channels/shared/`**，按使用它们的 feature 门控。策略留在
   渠道里；shared 模块只执行渠道要求的事。

8. **测试贴着代码放**在 `tests/<id>.rs`，用 `#![cfg(feature = "…")]` 门控。手工构造
   Provider 与凭证视图，用一份 fixture 密钥调 `prepare`，断言 URL、注入的鉴权和被丢弃的
   header。把抓到的帧喂给用量观察者；解析录下来的配额 header 与用量 body。一个脚本化的
   出站 client 用来演练多次调用覆写。**不要从这里测引擎。**

9. **把 feature 转发到每一个宿主。** 引擎从不自己列渠道：`gproxy-sdk` 注册
   `compiled_in()` 返回的全部渠道，所以一个渠道能否被某个二进制用到，恰好取决于那个二进制的
   feature 集合能否到达它的 feature。用同一个名字把 feature 加到：

   | 文件 | 要加的行 |
   | --- | --- |
   | `crates/gproxy-sdk/Cargo.toml` | `acme = ["gproxy-channel/acme"]`，并把 `"acme"` 加进它的 `channels` 列表 |
   | `crates/gproxy/Cargo.toml` | `acme = ["gproxy-sdk/acme"]` |
   | `crates/gproxy-host-edge/Cargo.toml` | `acme = ["gproxy-sdk/acme"]` |
   | `crates/gproxy-app/src/v3/channels.rs` | 把 id 加进 `V4_CHANNELS`，让 v3 导入接受它 |

   重复 id 会被拒绝，而点名了未注册渠道的 Provider 是一个配置错误，不是一次回退。

## 收尾

```sh
cargo fmt --all
cargo clippy -p gproxy-channel --all-features --all-targets -- -D warnings
cargo clippy -p gproxy-channel --all-features --target wasm32-unknown-unknown --lib -- -D warnings
cargo test   -p gproxy-channel --all-features
```

**每个渠道都要能为原生目标和 `wasm32-unknown-unknown` 构建**，这正是 Workers 宿主不是一个
缩水渠道集的原因。lint 报错要改代码，不是加 `#[allow]`。

进去之后就不需要别的了：新渠道上的 Provider 会带着它的 descriptor 出现在
`GET /admin/api/channels` 里，管理 UI 从那份 descriptor 渲染它的表单。

## 在本仓库之外

渠道可以放在自己的 crate 里，不用 fork 就能发布，两个宿主都行：一个原生二进制，它就是原版
`gproxy`——同样的命令、参数、控制台和数据库；或者一个 Worker，它就是原版 GPROXY Worker。
两者都只是在内置渠道旁边多注册了你的渠道。`examples/channel-plugin` 是一套完整、可运行的
三个 crate：一个文件实现的 OpenAI 兼容 API key 渠道，以及每个宿主几行代码的入口。

### 目录结构

```text
my-gproxy/
  channel/    渠道：一个实现 BaseChannel 的单元结构体，不依赖任何宿主
  native/     gproxy 二进制，带上它
  edge/       Cloudflare Worker，带上它（只在部署到那里时需要）
```

渠道单独成一个 crate，两个宿主才能都依赖它。原生宿主不能是它的依赖：`gproxy` 编不了
wasm，Worker 也就跟着编不了。

```rust
// native/src/main.rs
use std::{process::ExitCode, sync::Arc};

fn main() -> ExitCode {
    gproxy::main_with(vec![Arc::new(my_channel::Acme)])
}
```

`main_with` 就是原版 `main` 的全部：加载 `.env`、解析同样的参数、建运行时、运行。自己有
运行时的宿主改调 `gproxy::run_with(cli, channels)`。

```rust
// edge/src/lib.rs
#[cfg(target_arch = "wasm32")]
mod entry {
    use std::sync::Arc;

    use worker::{Context, Env, HttpRequest, event};

    #[event(fetch)]
    async fn fetch(
        request: HttpRequest,
        env: Env,
        ctx: Context,
    ) -> worker::Result<gproxy_host_edge::Response> {
        gproxy_host_edge::fetch_with(request, env, ctx, || vec![Arc::new(my_channel::Acme)])
            .await
    }
}
```

`fetch_with` 就是原版 fetch handler 的全部。一个 Worker 模块只能有一个 `#[event(fetch)]`，
所以 edge crate 依赖 `gproxy-host-edge` 时要**关掉**它的 `entry` feature——原版 handler
就是那个 feature 声明的。闭包在每个 isolate 里只运行一次，也就是第一个请求组装实例的时候。

两个宿主里，`id` 与某个内置渠道相同的渠道都会**替换**那个内置渠道——要发布一个改过的现有
渠道，就这么做。

### 依赖

按路径依赖一份 gproxy checkout，版本就是你要对着构建的那个：

```toml
# channel/Cargo.toml
[dependencies]
gproxy-channel = { path = "../../gproxy/crates/gproxy-channel" }
gproxy-protocol = { path = "../../gproxy/crates/gproxy-protocol", features = ["exhaustive"] }
http = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```toml
# native/Cargo.toml
[dependencies]
my-channel = { path = "../channel" }
gproxy = { path = "../../gproxy/crates/gproxy", features = ["channels", "memory", "fs", "embedded-console"] }
```

```toml
# edge/Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
my-channel = { path = "../channel" }
gproxy-host-edge = { path = "../../gproxy/crates/gproxy-host-edge", default-features = false, features = ["channels", "d1", "libsql"] }
worker = { version = "0.8", default-features = false, features = ["http"] }
```

```toml
# Cargo.toml，workspace 根
[workspace]
resolver = "3"
members = ["channel", "native", "edge"]

# 从 checkout 根目录的 Cargo.toml 复制：patch 只在声明它的 workspace 里生效，
# 而构建依赖这些。
[patch.crates-io]
sqlx-sqlite = { path = "../gproxy/vendor/sqlx-sqlite" }
btls-sys = { git = "https://github.com/0x676e67/btls", rev = "609f955e6de4df4a215ee9b273d327af1c0771db" }
```

第一次构建前把 checkout 里的 `Cargo.lock` 复制到根 `Cargo.toml` 旁边，这样解析出来的依赖
版本就是 gproxy 测试过的那些。

用 checkout 而不是 git 依赖，只有一个原因：原生控制台是从
`crates/gproxy-host-axum/assets/web` 编进二进制的，而那个目录没有提交。先在 checkout 的
`console/` 里跑 `pnpm install && pnpm build`；或者从 features 里去掉 `embedded-console`，
得到一个不带控制台的二进制。下面的 Worker 构建也从 checkout 取控制台。

`gproxy` 和 `gproxy-host-edge` 自己的 feature 列表和原版构建一样用：`channels` 是全部
内置渠道，也可以只点名部署要用的那些。

### 构建 Worker

checkout 里的 `deploy/cloudflare` 能构建任何一个 Worker crate，不只是原版那个。用绝对路径
指向你的 crate，然后照常部署：

```sh
cd ../gproxy/deploy/cloudflare
pnpm install
GPROXY_EDGE_CRATE=/absolute/path/to/my-gproxy/edge pnpm run build
pnpm run deploy
```

它会在你的 crate 里跑 `worker-build`、裁掉调试段，再把 Worker 复制到 checkout 的
`wrangler.toml`、控制台资源和它们的 header 旁边。[Edge 部署指南](/zh-cn/deployment/edge/)
里的一切——D1 绑定、密钥、`GPROXY_CONFIG`——原样适用。

**给 Worker 用的渠道必须能编到 `wasm32-unknown-unknown`**，每个内置渠道都已经遵守这条规则。
实际上就是：不用 `tokio` 网络、不碰文件系统、用 `web-time` 而不是 `std::time::Instant`，
所有请求都走宿主交过来的 client。用
`cargo check --target wasm32-unknown-unknown -p my-channel` 检查。

### 公开契约给了你什么

渠道要实现或会收到的一切都在 `gproxy_channel::channel` 里公开：`BaseChannel`、上下文与
视图类型、每一个能力 trait、descriptor 类型，以及构造请求要用的助手——处理客户端 header 的
`forwardable` 与 `HeaderAllowlist`、写入运维静态 header 的 `insert_configured`、清理查询串的
`strip_query_auth`，还有 `standard_reason_observer`。协议词汇（`Dialect`、`Operation`、
`OperationKey`、`HttpBody`、`WireRequest`）来自 `gproxy_protocol`。

`gproxy_channel::channels::shared` **不**公开：它是为本仓库里的渠道调的，会随它们一起变。
需要什么就复制什么。

上面的规则原样适用——尤其是准备必须是纯的、来源鉴权绝不到达上游。

### 与内置渠道的不同

- **控制台和对待其他渠道一样，从 descriptor 渲染你的渠道**：Provider 表单来自
  `config_keys`，凭证表单来自 `credential_kinds`，登录按钮来自 `login_modes`，配额、重置、
  刷新与付费用量开关来自 `capabilities`。它没有的是为内置 id 写的那些逐渠道润色——量身定制的
  提示文字、模型预设组。
- **原生二进制的自更新会拒绝安装。** 发布的 release 是本仓库的构建；把它换上来会丢掉你的
  渠道，下次重启后它上面的每个 Provider 都会失效。请对着新版本重新构建。检查更新照常可用。
- **契约跟着 gproxy 的版本走。** 实际上的变化都是增量的——大多数协议类型带
  `#[non_exhaustive]`，`BaseChannel` 的新方法都带默认实现——但请对着你运行的那个版本构建。
  写 `ChannelCapabilities { quota_query: true, ..Default::default() }` 而不是列出每个字段，
  这样新增一项能力不会让你的构建失败。

## 给编码 Agent 的清单

同样的步骤，按 agent 应当执行的方式列出。

**内置**（向本仓库提 PR）：

1. 以 `crates/gproxy-channel/src/channels/nvidia.rs`（API key）或 `codex/`（OAuth）为模板，
   并阅读 `samples/` 下厂商的 wire 源码或它的 API 参考。
2. 在 `crates/gproxy-channel/Cargo.toml` 里加 feature，在 `src/channels/mod.rs` 的
   `channels!` 里加一行，并把 feature 加进该渠道用到的 `src/channels/shared/` 里每一个
   `cfg(any(...))`。
3. 写 `src/channels/<id>.rs` 或 `src/channels/<id>/`；覆写 `descriptor`。
4. 在第 9 步列出的四处转发 feature。
5. 如果渠道认 `allow_paid_usage` 或提供 WebSocket，在它 descriptor 的 capabilities 里设
   `paid_usage` / `websocket`。
6. 在 `docs/src/i18n/reference/zh-cn.json` 和 `zh-tw.json` 中补齐新增 descriptor／Schema 说明的翻译，
   运行 `pnpm --dir docs generate:reference`。它会导出已注册渠道，缺少翻译时会失败。不要提交生成的 JSON。
7. 跑[收尾](#收尾)下的命令。

**在本仓库之外**：

1. 把 `examples/channel-plugin` 复制到 checkout 之外——`channel/` 必须要，`native/` 和
   `edge/` 按你要部署的宿主取。加上上面那个带 `[patch.crates-io]` 表的 workspace 根
   `Cargo.toml`，并复制 `Cargo.lock`。
2. 在每个 crate 的 `Cargo.toml` 里，用包自己的 `version` 和 `edition = "2024"` 替换
   `*.workspace = true` 这些键，把每个 `workspace = true` 依赖改成上面写的路径依赖。
3. 重命名包、二进制、结构体和 `ID`。除非就是要替换某个内置渠道，否则选一个内置渠道没用过的 id。
4. 只对着 `gproxy_channel::channel` 实现；不要去用 `channels::shared`。
5. `cargo clippy --all-targets -- -D warnings`；要部署 Worker 的话再跑
   `cargo check --target wasm32-unknown-unknown -p <channel> -p <edge>`。
6. 在一个全新的 `GPROXY_DATA_DIR` 上运行原生二进制，或者按上面的步骤构建后在
   `deploy/cloudflare` 里用 `wrangler dev --local` 运行 Worker，确认
   `GET /admin/api/channels` 里列出了这个 id。
