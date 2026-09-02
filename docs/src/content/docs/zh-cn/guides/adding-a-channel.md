---
title: 新增通道
description: "内置通道的结构、它实现的 Channel 契约、注册位置，以及控制台无需新增界面即可自动识别的内容"
---

通道是某一上游家族的适配器：它知道 URL、如何注入凭证、如何读取流、如何提取用
量，对 OAuth 类上游还知道如何登录与刷新。其余一切——路由、准入、故障转移、变
换、结算与捕获——都是引擎的职责，不在通道里重新实现。

## 内置，而非插件

v3 没有插件机制：没有 `linkme` slice，没有外部通道 crate，也没有按通道划分的
Cargo feature。每个通道都是 `crates/gproxy-channels` 的一个模块，编入二进制的
集合就是 `crates/gproxy-app/src/bootstrap.rs` 中的列表。新增通道意味着向本仓
库提交 pull request。28 个内置 id 构成运行时目录；`claudeweb` 是唯一限定原生
构建的通道。

## `Channel` 契约

契约位于 `crates/gproxy-channel-api/src/`。`Channel` 是同步且对象安全的：适配
器是作用于借用数据的纯逻辑，唯一的异步关注点——凭证刷新——返回一个装箱的
future。`prepare` 不得执行 I/O。

必须实现的方法：

| 方法 | 职责 |
| --- | --- |
| `descriptor()` | 身份卡：`id`、`display_name`、可执行的 `supports`、声明的 `provider_fields` 与 `credential_fields`、`endpoint_overrides`、`traffic_policy`。 |
| `routing_table()` | 创建 Provider 时播种的默认值：每个（操作，入站协议）一条 `ChannelSupport`，动作为 `passthrough`、`transform`、`local` 或 `unsupported`。 |
| `prepare(PrepareCtx)` | 构建绝对的上游请求：来自设置或端点覆盖的 URL、从解密后的机密注入认证、请求头允许列表、请求体整形。 |
| `classify(ResponseView)` | 把上游应答映射为 `Success`、`Retryable`、`Terminal` 或 `CredentialDead`；它驱动故障转移与健康。 |
| `extract_usage(UsageCtx)` | 从缓冲的交换中读取 `NormalizedUsage`：输入、输出与缓存 Token，加上维度化的 `metrics` 与 `dimensions`。 |

可选钩子，默认实现不做任何事：

| 钩子 | 使用场景 |
| --- | --- |
| `login()` | 通道以交互方式获取凭证。返回 `ChannelLoginRef`，其描述符列出模式（带 PKCE 的 `AuthCode`、`Device`、`Cookie`）与参数；适配器实现 `ChannelLogin`。 |
| `refresh_due()`、`refresh()` | 机密会过期。`refresh` 返回完整的替换机密；引擎通过宿主的版本守卫存储持久化它。 |
| `stream_decoder(StreamCtx)` | 线上是 SSE、AWS event-stream 或其他必须解码成帧并观察用量的分帧。返回 `StreamDecoder` 状态机：`push` 送入分块，输出 `Frame`，`finish` 产出带用量的 `StreamTail`。 |
| `shape_response()` | 通道私有的信封必须在向外变换之前规范化为声明的原生线上格式。 |
| `select_support()` | 同一凭证家族服务多条源行，由机密形状决定选哪条。 |
| `operation_driver()` | 一个操作需要多次上游调用（创建、轮询、获取）。驱动器是状态机；核心执行并把每次调用纳入漏斗。 |
| `observe_quota()`、`prepare_quota_probe()`、`parse_quota_probe()` 及 credits 与 reset 变体 | 上游在响应头中报告配额窗口，或提供用量端点。 |
| `session_preparer()` | 带用量计量的长连接实时会话。 |
| `settlement_ready()`、`resource_mutations()` | 异步操作以及必须记录归属的持久资源（文件、视频）。 |
| `surfaces()`、`prepare_surface()` | 通道模拟厂商控制平面（Codex `backend-api`、Claude Code 文件）。条目是表格行：方法、路径模式、凭证亲和，以及转发或合成。 |
| `requires_continuations()` | 通道依赖调用之间的续接状态。 |

`PreparedRequest` 携带请求，以及可选的流 `framing` 覆盖、`websocket` 标志和
可选的 `ClientProfile`：原生传输层应用的 TLS 与 HTTP/2 指纹
（`ClientProfilePreset::Chrome148` 是采集好的预设）。Edge 宿主忽略可选的指
纹。

## 声明的字段

控制台没有任何通道专属界面。它为通道渲染的一切都来自描述符，经由
`GET /admin/api/channels` 获取：

| 字段 | 用途 |
| --- | --- |
| `provider_fields` | 类型化的 Provider 设置。控件：`text`、`secret`、`url`、`integer`、`boolean`、`string_list`、`select`（带 `options` 与 `default_value`）；`required` 与 `advanced` 标志。 |
| `credential_fields` | 粘贴凭证时机密的形状：`api_key`；`access_token` 与 `refresh_token`；服务账号字段。 |
| `endpoint_overrides` | 设置标签页是否提供按操作的端点 URL 覆盖；键来自 `endpoint_override_key`。 |
| `traffic_policy` | 通道转发的请求头、响应头与查询参数；操作员可按 Provider 覆盖。 |
| `login` | 凭证向导的模式与参数。 |

尽量复用 `crates/gproxy-channels/src/metadata.rs` 中的字段集（`BASE_URL`、
`OPENAI_CACHE`、`CLAUDE`、`API_KEY`、`OAUTH`、`SERVICE_ACCOUNT` 等）。标签来
自语言文件：每个字段键都需要在
`console/src/locales/{en,zh-CN,zh-TW}/providers.json` 中提供
`providers.channelFields.<key>.label` 与 `.description`，`select` 选项需要
`providers.channelFieldOptions.<key>.<option>`。管理 API 会自行为每个通道加上
`auto_refresh_models`。

## 路由表

用 `shared/routing.rs` 中的 `route!` 宏声明路由：

```rust
use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, openai),
    route!(xform ListModels, claude => ListModels, openai),
    route!(local CountTokens, openai),
    route!(pass GenerateContent, openai_chat),
    route!(xform GenerateContent, claude_messages => GenerateContent, openai_chat),
    route!(unsupported CreateEmbedding, gemini),
];
```

线上类型中，`openai`、`claude`、`gemini` 用于家族操作，`openai_chat`、
`openai_responses`、`openai_responses_websocket`、`claude_messages`、
`gemini_generate_content` 用于内容生成。`xform` 行必须命名变换注册表已实现的
变换对；`crates/gproxy-core/src/tests/channels.rs` 中的测试
`every_declared_builtin_transform_is_wired` 会对描述符的 `supports` 做此检查，
声明了变换的新通道应加入它的列表。之后操作员可以按 Provider 覆盖任意一行
（见[路由规则与规则集](/zh-cn/guides/rules/)）。

## 通道的位置

每个通道 id 一个目录，每个关注点一个文件，任何文件不超过 500 行，最好少于
200 行：

```text
crates/gproxy-channels/src/<id>/
  mod.rs        descriptor, SUPPORTS, Channel impl
  routes.rs     routing_table()
  prepare.rs    URL, auth, endpoint overrides
  model.rs      model id and body shaping
  sse.rs        stream decoder
  usage.rs      usage extraction
  resource.rs   settlement_ready / resource_mutations (when needed)
  login.rs      ChannelLogin (when needed)
  auth.rs       refresh_due / refresh (when needed)
  quota.rs      quota probe (when needed)
  surface/      service-surface table and synthesizers (when needed)
  tests.rs      or tests/ for larger suites
```

共享的线上知识放在 `crates/gproxy-channels/src/shared/` 下：`openai`、
`claude`、`gemini`、`aws_eventstream`、`code_assist`、`google_oauth`、`cache`
（魔法字符串）、`quota`、`http`、`image_multipart`。`policy.rs` 保存每个通道
的 `ChannelTrafficPolicy`，`metadata.rs` 保存字段集，`legacy.rs` 把旧 id 下导
入的设置规范化。

API 密钥类通道可参考 `crates/gproxy-channels/src/openai/`，带登录、刷新与服务
界面的 OAuth 通道可参考 `claudecode/`。线上格式的依据是厂商的 API 文档，而不
是另一个通道的代码。

## 注册

1. 在 `crates/gproxy-channels/src/lib.rs` 中加入 `mod <id>;` 与
   `pub use <id>::<Name>Channel;`。
2. 在 `crates/gproxy-app/src/bootstrap.rs` 的 `channels()` 列表中加入
   `Box::new(gproxy_channels::<Name>Channel)`。`ChannelRegistry::new` 遇到重
   复 id 时会让启动失败。
3. 如果通道无法为 `wasm32-unknown-unknown` 构建，像 `claudeweb` 那样用
   `#[cfg(not(target_arch = "wasm32"))]` 同时限定模块与注册。优先编写两个目
   标都能构建的代码。
4. 为新增的字段键补充语言文件条目。

此外无需其他工作：创建 Provider 时会从 `routing_table()` 播种路由规则，供应
商页面会列出该通道，凭证向导会跟随 `login()`。

## 测试

通道测试与代码放在一起（`tests.rs` 或 `tests/`）。测试容易出错的部分：基于固
定机密的请求准备、基于采集帧的流解码与用量提取、配额解析，以及 `supports` 与
`routing_table()` 的一致性。不要在通道里测试引擎。以 `cargo fmt`、
`cargo clippy` 和 `cargo test` 收尾；lint 告警要修改代码，而不是加
`#[allow]`。
