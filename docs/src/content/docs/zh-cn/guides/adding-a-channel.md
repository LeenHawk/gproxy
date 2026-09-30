---
title: 新增通道
description: "一个 v4 渠道的结构：BaseChannel 契约、可选能力 trait、模块布局，以及从一个 Cargo feature 到注册完成的九步。"
---

一个渠道只知道一族上游：它的 URL、凭证怎么注入、它讲哪些线方言、它的流怎么报 usage，
以及——对账号型上游——怎么登录、刷新和读配额。

其余都是别人的事。路由、凭证选择、失败转移、协议转换、结算和 capture 属于引擎，而且
**渠道里没有任何东西读数据库或挑传输后端**。

渠道是编译进去的，不是插上去的：新渠道是 `gproxy-channel` 的一个模块，藏在它自己的 Cargo
feature 后面，新增一个意味着向本仓库提一个 PR。默认不编译任何具体渠道。

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
| `usage_extractor` | 从缓冲的响应里取归一化用量。`None` 表示**未报告**，不是 0 |
| `usage_stream` | 按响应的观察者，喂给它原始 chunk 或帧。它绝不改写交付出去的流 |
| `services` | 厂商控制面路由——见 [CLI 客户端](/zh-cn/guides/cli-clients/) |

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

9. **在宿主里注册它。** 引擎从不自己列渠道。想要全部编译进去的宿主就取整份列表——
   `gproxy-sdk` 正是这么做的，所以在那边打开 feature 就是唯一一步——想要精确集合的则传实例
   进去。重复 id 会被拒绝，而点名了未注册渠道的 Provider 是一个配置错误，不是一次回退。

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
