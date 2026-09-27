# gproxy-channel

[English](README.md) | 简体中文

GPROXY v4 的上游适配层。一个渠道只认识一个上游家族：它的 URL、凭证如何注入、
原生接受哪些 wire dialect、怎样把回答整形成该操作的标准响应（宿主就从这里读用量），
以及账号类上游如何登录、刷新和读取额度。其余一切（选路、凭证选择、失败转移、协议转换、结算、捕获）都属于
`gproxy-core`；这里不读数据库，也不选择传输。

本 crate 依赖 `gproxy-protocol` 取操作与 wire 类型，依赖 `gproxy-client` 只为
`OutboundClient` 契约并重导出它。默认不编译任何具体渠道，每个渠道对应一个
Cargo feature：

| feature | id | 上游 | 凭证 |
|---|---|---|---|
| `aistudio` | `aistudio` | Google AI Studio：同一 origin 上的原生 Gemini 方法与 `/v1beta/openai` 兼容层、SSE 与 JSON 数组两种流、`usageMetadata` 计量 | `{"api_key"}` |
| `antigravity` | `antigravity` | 经 Antigravity 编辑器所用的 Code Assist 主机使用的 Google 账号：PKCE 登录并在登录时发现 Cloud project 与档位、刷新、Code Assist 请求信封、`fetchAvailableModels` 目录与逐模型额度 | `OAuthCredential` |
| `aws_bedrock` | `aws_bedrock` | AWS Bedrock：`bedrock-runtime` 上经 SigV4 签名的 `InvokeModel`（Anthropic 模型），AWS event-stream 响应翻译成 Claude Messages SSE，控制面的基础模型目录 | AWS 访问密钥对（可为临时）或 Bedrock API key |
| `azure` | `azure` | Azure OpenAI 以及 Azure AI Foundry 同时托管的 Anthropic 模型：资源下的 v1 面或 deployment 面，`api-key` 与 `x-api-key` | `{"api_key"}` |
| `claudeapi` | `claudeapi` | Anthropic 自家 API：`x-api-key` 的 Messages，含该 API 要求的请求 hygiene 与服务端 fallback、OpenAI SDK 兼容层、`anthropic-ratelimit-*` 头、`/v1/organizations/cost_report` | `{"api_key", "quota_api_key"}` |
| `claudecode` | `claudecode` | 经 Claude Code CLI 的 Messages 请求使用的 Claude.ai 订阅：PKCE 与 cookie 登录、刷新、统一限速头、`/api/oauth/usage`、CLI 服务 | `OAuthCredential` |
| `claudeweb` | `claudeweb` | claude.ai 浏览器会话：对 `/api/bootstrap` 的 cookie 登录、多次调用组成的对话轮次翻译成 Claude Messages SSE、组织级用量窗口 | 会话 cookie + 组织 |
| `cline` | `cline` | `api.cline.bot` 上 Cline 自己的账号：WorkOS 设备登录后再向 Cline 注册、带 `workos:` 前缀的 bearer、SDK 的归属头、响应外面那层 `{success, data}` 信封、它自己的推荐模型分组、套餐窗口与信用点余额 | `{"api_key"}` 或 `OAuthCredential` |
| `codex` | `codex` | 经 Codex 后端使用的 ChatGPT 账号：OAuth（PKCE 与 device code）、HTTP SSE 与 WebSocket 上的 Responses、`x-codex-*` 限额头、`/wham/usage`、CLI 后端服务 | `OAuthCredential` |
| `copilotcli` | `copilotcli` | 经 `copilot` CLI 使用的 GitHub Copilot：GitHub 设备登录拿到的长期令牌由 `CredentialRefresh` 换成短期 Copilot 令牌、CLI 的编辑器身份头、按席位决定的 origin、`copilot_internal/user` 的额度快照 | GitHub OAuth 令牌（+ 换来的 Copilot 令牌） |
| `custom` | `custom` | 任何原生讲 OpenAI／Claude／Gemini 的 API-key 端点 | `{"api_key"}` |
| `dashscope` | `dashscope` | 阿里云百炼 DashScope：同一 origin 上的 OpenAI 兼容面、Anthropic 兼容面、独立的 rerank 前缀，以及原生的多模态生成图像 API | `{"api_key"}` |
| `deepseek` | `deepseek` | DeepSeek：`/v1` 下的 Chat Completions、根路径上的 Responses、`/anthropic` 下的 Claude Messages、`prompt_cache_hit_tokens`、`/user/balance` | `{"api_key"}` |
| `devin` | `devin` | `server.codeium.com` 上的 Devin（Windsurf）：传输是 Connect-RPC + protobuf 而非 JSON，`GetChatMessage` 的多帧流翻译成 Chat Completions SSE，`GetUserStatus` 给日／周两个窗口 | 会话 token |
| `geminicli` | `geminicli` | 经 Gemini CLI 所用的 Code Assist 端点使用的 Google 账号：PKCE 登录并在登录时发现 Cloud project 与档位、刷新、Code Assist 请求信封、`retrieveUserQuota` 目录与逐模型额度 | `OAuthCredential` |
| `grokbuild` | `grokbuild` | 经 Grok Build CLI 使用的 xAI 账号：`auth.x.ai` 的设备登录与刷新、打在 `cli-chat-proxy.grok.com` 上的收窄版 Responses body、留在 `api.x.ai` 的 xAI 自有媒体路径、`/billing?format=credits` | `OAuthCredential` |
| `kimi` | `kimi` | 月之暗面：用 API key 走 `api.moonshot.cn` 平台，或用设备登录走 `api.kimi.com` 的 Kimi Code 订阅（刷新、CLI 的 `x-msh-*` 身份头）；额度是 `/usages` 窗口或现金余额 | `{"api_key"}` 或 `OAuthCredential` |
| `kiro` | `kiro` | 经 Kiro 桌面端使用的 AWS CodeWhisperer：设备登录或 IAM Identity Center 登录、刷新、把 Responses 请求改写成 CodeWhisperer 会话信封、把 AWS event-stream 回包翻译回 Responses SSE、`GetUsageLimits` 的额度窗口 | `OAuthCredential` |
| `openai` | `openai` | OpenAI 自家平台：完整 OpenAI 面、WebSocket 上的 Responses 与 Realtime、`x-ratelimit-*` 头、`/v1/organization/costs` | `{"api_key", "quota_api_key"}` |
| `opencode` | `opencode` | OpenCode Zen 与 Go：每档一个 origin，上面同时挂 Chat Completions、Responses 与 Claude Messages 三个面，CLI 的 `x-opencode-session` 会话头、Console 设备登录，Go 档的 `/usage` 窗口 | `{"api_key"}` 或 `OAuthCredential` |
| `openrouter` | `openrouter` | OpenRouter：把 provider 选路偏好填进 body、`HTTP-Referer`／`X-Title` 归属头、响应里自报的价格、`/v1/auth/key` | `{"api_key"}` |
| `vertex` | `vertex` | Google Vertex AI：按项目与地区寻址 google／anthropic／OpenAI 兼容三个发布者；服务账号密钥经 `CredentialRefresh` 换取访问令牌 | Google 服务账号密钥 |
| `vertexexpress` | `vertexexpress` | Vertex AI Express 模式：单一全局 origin 上的 Gemini 面，key 走 query，无项目无地区 | `{"api_key"}` |
| `workbuddy` | `workbuddy` | 经编辑器插件使用的腾讯 Copilot：设备登录与刷新、`/v2` 下带插件身份头与账号头的 OpenAI Chat、`/v3/config` 目录、企业与个人两套计费表 | `OAuthCredential` |
| `xai` | `xai` | xAI（Grok）：OpenAI Chat 与 Responses，外加 xAI 自己的 `/v1/tts`、`/v1/stt`、`/v1/videos/generations`，`cost_in_usd_ticks` 计量，管理面的账单探测 | `{"api_key"}` |

所有渠道都能在原生目标和 `wasm32-unknown-unknown` 上构建。

`claudeapi`、`openai`、`aistudio` 是三家厂商自己的第一方 API。它们成为渠道而不是
一个 `custom` Provider，理由都是 `custom` 表达不了的路由布局：Anthropic 的路由由
operation 而不是客户端 path 决定，OpenAI 的 Responses 与 Realtime 要走 socket，
AI Studio 在同一个 origin 上放了两套面、各要各的凭证头。

`dashscope`、`deepseek`、`kimi`、`openrouter`、`xai` 是 API-key 舰队：它们的 wire
就是那三种兼容形状之一，因此共用 `channels::shared::compatible` 做有界的能力调用，
彼此的差别只在方法落在哪里、usage 对象多带了哪些字段（各渠道的 `UsageExtras`）、
账号面报什么。
每一个都因为"`custom` 表达不了的东西"才成为渠道——表达得了的那些见下文
《不需要渠道的厂商》。

其中两个报的是价格而不只是计数。`openrouter` 在开启用量记账时记录
`upstream_cost_usd`，`xai` 在视频任务上记录它（那类响应自报美元），两者同时打上
`upstream_priced` 维度。Core 按仓库里的费率行计价、并不认识这些指标的名字，所以
运营方若想让上游自报的数字就是账单，就为 `upstream_cost_usd` 写一条 1 USD／单位
的费率行、不写 token 行。xAI 的 `cost_in_usd_ticks` 按原名记录，**刻意不是美元**：
那是 xAI 自己的计量单位，要自己的费率行。

`azure`、`vertex`、`vertexexpress` 是云上的转售方：它们原样转发所托管厂商的 wire，
只改方法的位置和凭证的呈现方式。三者都不伪装任何客户端，因此都不返回
`default_connection`——出站栈只由运营方的连接 profile 决定。`vertex` 的凭证入库时
`expires_at_ms` 已经过期，宿主的第一次刷新就会铸出访问令牌；`prepare` 从不铸令牌。

`aws_bedrock` 是唯一"签名"而不是"出示令牌"的渠道：每个请求都带 AWS SigV4 的
`Authorization`，签名在 `prepare` 里算——它是对请求和凭证的纯算术，没有 I/O。
它的流式响应不是 SSE 而是 AWS event-stream 分帧，所以 `stream_generate_content`
被覆写，把帧翻译成客户端要的 Claude Messages SSE。它服务 Anthropic 系模型；
其余家族要走 Bedrock 的 `Converse` 形状，而 `gproxy-protocol` 目前无法表达它。

`geminicli` 与 `antigravity` 是 Gemini 家族的 CLI 仿真面：用 Google 账号的 OAuth 凭证
打这两个工具各自使用的 Code Assist 内部端点，而不是 Gemini API key。两者共用 Code Assist
请求信封、Google 登录以及登录时一次性的 project 与档位发现，区别在 client id、scope、
user agent、主机和目录方法。

`cline`、`copilotcli`、`opencode` 是三个有自己账号体系的编码工具，不是厂商。它们各自把
很多模型挡在一条自己没有发明的 wire 后面，所以渠道存在的理由全在请求周围：一个从 A 处
开始却在 B 处结束的设备登录、一份推理端点根本不收的凭证、一层包着回答的信封、一个不是
模型列表的目录、一个和流量不在同一台主机上的账号面。三者里只有 `copilotcli` 有值得复现的
客户端身份（v3 只给它写了 `profile.rs`），所以也只有它返回 `default_connection`。

`copilotcli` 是唯一"拿到的凭证不能直接用"的渠道。GitHub 设备登录给出的是长期 GitHub OAuth
令牌，而 Chat Completions 只收由它换来的、有效期以分钟计的 Copilot 令牌。这次交换被建模成
`CredentialRefresh`——GitHub 令牌是 refresh token，Copilot 令牌是 access token——因为
`prepare` 是同步且纯的，放在那里会每个请求都换一次。登录的最后一步先换一次，宿主落库的
凭证因此立刻可用。

`kiro`、`workbuddy`、`grokbuild` 是另外三个 CLI 仿真面，彼此只共享这个形状：一家厂商
的编码工具、它的账号登录、以及它发的身份头。`kiro` 离公开 API 最远——CodeWhisperer
收的是 Smithy JSON 的会话信封，回的是 AWS event-stream 分帧，所以渠道把 Responses body
改写成信封，再把回包翻译回 Responses SSE。`workbuddy` 与 `grokbuild` 讲的是 OpenAI，
它们的活是身份与收窄：WorkBuddy 的账号头块与网关信封，Grok Build 的双 origin 与它的
chat proxy 拒收的那批 Responses 字段。三者都读客户端自己发的 session，而不是新铸一个：
`design/session-identity.md` 点名了 WorkBuddy 的 `x-conversation-id` 与 Grok Build 的
`x-grok-session-id`，也点名了它们旁边那些按请求／按轮次生成的 id **不是** session。
Kiro 在那张阶梯表里没有条目，它的客户端也不发。

## 不需要渠道的厂商

一个渠道就是一份要跟着别人 wire 维护的代码。只有当"Provider 行表达不了它要的
东西"时，厂商才值得一个渠道：路径随 operation 移动、body 必须改写、有账号面要读、
有客户端身份要出示。差别只有一个 origin 加一个头的厂商，就是一个 `custom`
Provider——下面三个正是如此。这些配方就是完整的 Provider 行，`base_url` 是
Provider 列，其余都在它的 `config` JSON 里。

**NVIDIA NIM**——一个 OpenAI 兼容端点，仅此而已。

```json
{ "base_url": "https://integrate.api.nvidia.com", "config": { "dialects": ["openai_chat"] } }
```

**Vercel AI Gateway**——和 OpenRouter 一样是一个 origin 挡在很多厂商前面，但
body 里没有选路对象、响应里也没有价格：没有任何东西要渠道去填或去读回来。
`custom` 按家族各用各的鉴权方式，正是这个网关接受的方式。

```json
{
  "base_url": "https://ai-gateway.vercel.sh/v1",
  "config": { "dialects": ["openai_chat", "openai", "claude"] }
}
```

**Cloudflare AI Gateway**——对一个确定的网关来说，account id 与 `/ai` 前缀是固定
的，所以它们属于 `base_url`；gateway id 是一个静态头。

```json
{
  "base_url": "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai",
  "config": {
    "dialects": ["openai_chat", "openai", "claude"],
    "headers": { "cf-aig-gateway-id": "default" }
  }
}
```

这三者都保留 `custom` 的魔法缓存开关与 `allowed_headers`，并按上游实际回的那种
兼容形状读取用量。


## 契约

`BaseChannel` 是唯一必须实现的 trait，`id` 是它唯一必须实现的方法。协议里的每个
`Operation` 都有自己的异步方法和默认实现：用 `prepare`（HTTP）或 `prepare_connect`
（WebSocket）构造请求，再交给指派的 client。两个准备钩子默认返回
`UnsupportedOperation`，所以渠道支持的恰好就是它准备或覆写的那些操作，不多不少。

```text
宿主选定 provider、credential、client
  → ChannelBinding::new(&channel, provider, credential, client)
  → binding.send(OperationKey, WireRequest<HttpBody>)
    binding.connect(OperationKey, WireRequest<()>)
  → BaseChannel 上对应名字的操作方法
  → 默认：prepare + client.send / prepare_connect + client.connect
    或渠道自己经同一个 client 完成的多次调用流程
  → WireResponse<HttpBody> / UpstreamConnection
```

宿主交给渠道的视图都是借用的，能公开的字段都公开：

| 类型 | 内容 |
|---|---|
| `ProviderView` | `id`、`channel`、可选 `base_url`、JSON `config`（渠道解码成自己的类型化设置） |
| `CredentialView` | `id`、`provider_id`、`auth_kind`、JSON `secret`（无 `Debug`、无 `Serialize`）、宿主记录的公开 `metadata`（套餐、账号 id）、`version`、`expires_at_ms` |
| `OperationContext` | 上述两个视图、操作的 `Dialect`、`WireRequest`、拥有所有权的 `Arc<dyn OutboundClient>`、`ChannelState`、宿主 `instance_id`、可选 `endpoint_override` |
| `CredentialContext` | 刷新、额度和服务用的 provider、credential、client |

每个渠道都遵守的规则：

- 准备阶段是同步纯逻辑：不做 I/O，不藏状态，不自建 client。操作覆写可以做多次
  交换，但只能经 `context.client`，并且可以留着它在响应流结束后收尾（比如释放
  一个厂商侧对话）。
- 源请求的鉴权永远到不了上游。`forwardable` 丢弃逐跳头、`host`、`content-length`、
  `authorization`、`x-api-key`、`x-goog-api-key`、`api-key`；渠道从凭证注入自己的
  鉴权。provider 的 `config.allowed_headers` 收窄其他客户端头的转发；`content-type`
  与渠道声明的 `ChannelHeaders`（厂商 CLI 自己的头）总是放行。
- `endpoint_override` 一旦设置就是完整的方法 URL，替代 `base_url` 加渠道默认路径。
- 响应保留 status、headers 和惰性 body；非 2xx 也是响应，不是错误。
  `ChannelError::UpstreamResponse` 用于能力调用（登录、刷新、额度）以及多次调用
  覆写里的中间交换，即失败的那个应答不是要返回的响应。
- `RefreshRejected` 表示上游明确拒绝了该凭证（`invalid_grant`、已吊销），宿主会将
  其标记为死亡。瞬时的传输故障和 5xx 必须以 `Transport` 抛出，宿主稍后重试。
- `ContinuationElsewhere { instance_id }` 表示一条活跃的上游连接被另一个宿主进程
  持有；宿主改路由，凭证本身没有问题。

## 可选能力

每种能力是独立 trait，由 `BaseChannel` 上默认返回 `None` 的访问器给出。渠道只实现
上游具备的那些，能力之间没有强绑定（能报告额度窗口的渠道不必提供重置）。

| 访问器 | trait | 用途 |
|---|---|---|
| `credential_refresh` | `CredentialRefresh` | 产出完整的替换 secret 与过期时间；宿主以 `version` 做 CAS 持久化 |
| `oauth_authorization_code` | `OAuthAuthorizationCode` | `authorize` 用宿主给的 PKCE challenge 与 state 组 URL；`exchange` 把 code 换成 `OAuthCredential` |
| `oauth_device_code` | `OAuthDeviceCode` | `start` 与单步 `poll`；调度归宿主 |
| `cookie_login` | `CookieLogin` | 把粘贴的 cookie 换成 `AcquiredCredential`（secret 加公开 metadata） |
| `quota_model` | `QuotaModel` | 同步纯函数：凭证具有的 `QuotaDimension`，由 `auth_kind` 与 metadata 推出 |
| `quota_query` | `QuotaQuery` | 从上游用量端点读 `QuotaSnapshot` |
| `quota_headers` | `QuotaHeaders` | 把响应头变成 `QuotaEntry`；空表示没有报告 |
| `quota_reset` | `QuotaReset` | 在售卖额度的上游上查看 credits 并手动重置 |
| `usage_extras` | `UsageExtras` | 厂商自己的用量字段（上游报的价格、自有名字的缓存计数、实际服务的模型），在宿主从渠道整形后的标准响应里读出标准用量之后补充 |
| `services` | `ChannelServices` | 厂商控制面路由（Codex 插件、文件、远程控制；Claude Code 文件），`ServiceCaller` 由宿主实现，提供身份、角色、用量与资源绑定 |

`ChannelState` 是宿主按 provider 与 credential 划定范围的跨请求记忆（要恢复的网页
对话、要复用的会话）。渠道用自己选的 key 经 compare-and-swap 写入；`NoState` 是
绑定的默认值，拒绝一切写入。

两种 OAuth 流都带同一个"口袋"，装渠道在用户完成授权之后还需要的事实：
`AuthorizationStart::provider_state` 原样回到 `AuthorizationCode::provider_state`，
`DeviceAuthorization::provider_state` 则随授权一起回到 `poll`。形状由渠道自己定——
非标准的设备句柄，或某次登录替自己注册的客户端。与被宿主当作凭证 metadata 公开的
`OAuthCredential::provider_fields` 不同，**这个口袋可以装 secret**：它只存在于宿主的
登录会话里，短命、落在 cache 上、从不被渲染，登录一结束就没了。要活过登录的东西必须
由 `exchange` 返回——公开事实进 `provider_fields`，secret 进
`OAuthCredential::provider_secrets`，后者与 token 一同封存，永远不会变成 metadata。

## 添加一个渠道

渠道是内建的，不是插件：新渠道就是本 crate 里一个由自己的 feature 门控的模块。
下面以 `custom`（API key）和 `codex`（OAuth 账号）为两个参考，按步骤说明。

1. 在 `Cargo.toml` 声明 feature。只列该渠道需要的可选依赖；仅 wasm 需要的依赖放在
   `[target.'cfg(target_arch = "wasm32")'.dependencies]`，如 `claudeweb` 的
   `send_wrapper`。

   ```toml
   [features]
   # 一行说明上游是什么、渠道覆盖到哪。
   acme = ["dep:base64", "dep:web-time"]
   ```

2. 往 `src/channels/mod.rs` 的 `channels!` 列表里加一行：feature、模块、宿主要注册的
   那个值。这一行同时声明模块并把渠道放进 `channels::compiled_in()`，宿主就是从那里
   找到它的。若用到 `shared`，把该模块的 `cfg(any(...))` 也加上——它自有一份列表。

   ```rust
   "acme" => acme, acme::Acme;
   ```

3. 按关注点分文件。小渠道一个 `acme.rs` 即可，大渠道用目录：

   ```text
   src/channels/acme/
     mod.rs        ID、单元结构体、重导出、写明 wire 事实来源的模块文档
     config.rs     AcmeConfig：provider `config` JSON，serde(default)，忽略未知键
     request.rs    impl BaseChannel：prepare / prepare_connect / 覆写的操作
     oauth.rs      OAuthAuthorizationCode、OAuthDeviceCode、CookieLogin、CredentialRefresh
     quota.rs      QuotaModel、QuotaQuery、QuotaHeaders
     usage.rs      UsageExtras、UsageSource
     services.rs   ChannelServices 路由与处理
   ```

   模块文档写明每条 wire 事实的出处（厂商 API 文档或 `samples/` 下的 CLI 源码）。
   wire 事实永远不来自另一个渠道的代码。

4. 实现 `BaseChannel`。最少是 `id`、`native_dialects` 和 `prepare`；结构体是无状态
   单元类型，一个实例服务该渠道的所有 provider。同时覆写 `descriptor`：默认实现只会
   读能力访问器，而显示名和该渠道从 provider `config` JSON 里解出的键只有渠道自己
   知道。管理界面渲染 Provider 表单就只有这份描述符可用——开了 `ts` feature 它还是
   一个 TypeScript 类型——漏写一个键，就等于没人能设置它。

   ```rust
   use gproxy_channel::channel::{
       BaseChannel, ChannelError, HeaderAllowlist, PrepareContext, ProviderView, forwardable,
   };
   use gproxy_protocol::{Dialect, HttpBody, Operation};
   use http::{HeaderValue, header};
   use serde::Deserialize;

   pub const ID: &str = "acme";

   #[derive(Debug, Default, Deserialize)]
   #[serde(default)]
   pub struct AcmeConfig {
       pub headers: std::collections::BTreeMap<String, String>,
   }

   #[derive(Debug, Default, Clone, Copy)]
   pub struct Acme;

   impl BaseChannel for Acme {
       fn id(&self) -> &'static str {
           ID
       }

       fn native_dialects(&self, _provider: ProviderView<'_>, _operation: Operation) -> Vec<Dialect> {
           vec![Dialect::OpenAiChat]
       }

       fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
           let config: AcmeConfig = serde_json::from_value(ctx.provider.config.clone())
               .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
           let key = ctx.credential.secret["api_key"]
               .as_str()
               .filter(|k| !k.is_empty())
               .ok_or(ChannelError::InvalidCredential)?;
           let base = ctx.provider.base_url.unwrap_or("https://api.acme.example/v1");
           let uri = match ctx.endpoint_override {
               Some(url) => url.to_owned(),
               None => format!("{}{}", base.trim_end_matches('/'), ctx.request.path),
           };
           let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
           let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
           headers.insert(
               header::AUTHORIZATION,
               HeaderValue::from_str(&format!("Bearer {key}"))
                   .map_err(|_| ChannelError::InvalidCredential)?,
           );
           let _ = config.headers; // 静态头、query 处理、body 整形放这里
           let mut builder = http::Request::builder().method(ctx.request.method).uri(uri);
           *builder.headers_mut().expect("fresh builder") = headers;
           builder
               .body(ctx.request.body)
               .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
       }
   }
   ```

   当上游需要多次交换、本地合成应答（从静态目录返回 `list_models`）、不同的传输，
   或把响应改写成声明的原生 dialect 时，覆写操作方法而不是 `prepare`。`codex`
   覆写 Responses 相关操作以加入 CLI 身份头并回放 turn-state；`claudeweb` 覆写
   `generate_content` 与 `stream_generate_content`，用多次调用驱动一次浏览器对话。

5. 声明厂商客户端会发什么。若渠道模仿某个 CLI，导出一个 `ChannelHeaders` 常量并用
   `HeaderAllowlist::from_view_for(provider, HEADERS)` 构造允许列表，这样 provider 的
   `allowed_headers` 不会剥掉 CLI 自己的头。把渠道的身份头传给 `forwardable` 的
   `also_drop`，客户端就无法伪造。上游会对客户端做指纹识别时返回
   `default_connection()`（`codex` 用原生 TLS 的 reqwest，`claudeweb` 用浏览器
   preset）；credential 或 provider 上显式指定的宿主 profile 仍然优先。

6. 把能力做成独立类型，从访问器返回。遵守各 trait 的规则：refresh 返回完整替换而
   不是合并；`QuotaModel::dimensions` 从 `credential.metadata` 读套餐事实而不走网络；
   渠道自己不读用量——它把回答整形成标准响应，`UsageExtras` 只补充厂商在标准 usage
   对象之外报的字段；`RefreshRejected` 只用于上游的明确拒绝。登录发现的公开
   事实（套餐、账号 id）放进 `AcquiredCredential::metadata` 或
   `OAuthCredential::provider_fields`，由宿主写到凭证行上；刷新还需要的 secret
   （某次登录自己注册的 client secret）放进 `OAuthCredential::provider_secrets`，
   它与 token 一同封存，不会被当作 metadata 公开。

7. 可复用的 wire 机制放到 `src/channels/shared/`（缓存魔法串、服务调用方辅助），按
   用到它的 feature 门控。策略留在渠道里；shared 模块只执行渠道要求的事。

8. 测试放在 `tests/<id>.rs`，加 `#![cfg(feature = "...")]`。手工构造 `ProviderView`
   与 `CredentialView`，用夹具 secret 调 `prepare`，断言 URL、注入的鉴权和被丢弃的
   头；用 `support::settled` 结算整形后的回答，看宿主将读到的用量；解析录下来的额度头。脚本化的
   `OutboundClient`（见 `tests/capabilities.rs`）用来测多次调用的覆写，
   `tests/support/mod.rs` 里有脚本化的 `ServiceCaller`。不要在这里测 core。

9. 在宿主注册。core 自己不列举渠道。想要"编进来的都要"的宿主直接取整份列表——
   `gproxy-sdk` 就是这么做的，所以在那边开 feature 就是全部步骤；想要精确一组的宿主
   逐个传实例：

   ```rust
   let mut registry = ChannelRegistry::new();
   for channel in gproxy_channel::channels::compiled_in() {
       registry.register(channel)?;
   }
   let core = CoreBuilder::new(store).channels(registry).build().await?;

   // 或者逐个点名：
   let core = CoreBuilder::new(store)
       .channel(Arc::new(gproxy_channel::channels::custom::Custom))?
       .channel(Arc::new(gproxy_channel::channels::acme::Acme))?
       .build()
       .await?;
   ```

   `ChannelRegistry` 拒绝重复 id；provider 指向未注册的渠道是配置错误，不会回退到
   别的渠道。

最后跑 `cargo fmt --all`、`cargo clippy -p gproxy-channel --all-features
--all-targets -- -D warnings`、同样命令加 `--target wasm32-unknown-unknown --lib`，
以及 `cargo test -p gproxy-channel --all-features`。lint 报告要改代码，不加 `#[allow]`。
