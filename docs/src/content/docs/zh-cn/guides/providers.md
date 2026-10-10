---
title: "Provider 与凭证"
description: "选择渠道、添加供应商与凭证，配置登录、额度查询和连接选项。"
---

**渠道（channel）** 是编译进二进制的某一族上游适配器。**Provider** 是某个渠道上的一条
已保存连接：名字、可选 base URL、该渠道自己的 `config` JSON，以及一个凭证池。同一个渠道
可以建任意多个 Provider——`openai-main` 和 `openai-eu` 可以都在 `openai` 渠道上。

供应商和凭证的配置保存后生效，通常不需要重启。首次接入请先测试凭证，再把它加入模型路由。

## 支持的渠道

只有编译进你的二进制的渠道才存在。`GET /admin/api/channels` 回答自二进制而非数据库，
每一项带着登录方式、能力，以及一个 Provider 表单该渲染的 `config` 键。

| 渠道 | 上游 | 凭证 |
| --- | --- | --- |
| `aistudio` | Google AI Studio：原生 Gemini 方法与 `/v1beta/openai` 兼容层同源 | `{"api_key"}` |
| `antigravity` | 经 Antigravity 编辑器所用的 Code Assist 主机访问 Google 账号 | OAuth |
| `aws_bedrock` | AWS Bedrock：按模型选接口——Anthropic 模型走 `InvokeModel`（event-stream 翻成 Claude SSE），GPT、Grok、Qwen、DeepSeek 等走 OpenAI 兼容的 Chat Completions | AWS 密钥对，或 Bedrock API key |
| `azure` | Azure OpenAI，以及 Azure AI Foundry 托管的 Anthropic 模型 | `{"api_key"}` |
| `claudeapi` | Anthropic 官方 API，加上它的 OpenAI 兼容层与成本报表；未限定到单个 workspace 的密钥必须填写 `workspace_id` | `{"api_key", "quota_api_key"?, "workspace_id"?}` |
| `claudecode` | 经 Claude Code CLI 的请求使用 Claude.ai 订阅 | OAuth |
| `claudeweb` | claude.ai 浏览器会话，渲染成 Claude Messages SSE | 会话 cookie + 组织 |
| `cline` | Cline 自家账号，`api.cline.bot` | `{"api_key"}` 或 OAuth |
| `cloudflare_ai_gateway` | 经 REST API 使用 Cloudflare AI Gateway；account 与 gateway 按凭据存放，额度余额 | `{"api_key", "account_id", "gateway_id"?}` |
| `codex` | 经 Codex 后端使用 ChatGPT 账号：HTTP SSE 与 WebSocket 上的 Responses、`/wham/usage` | OAuth |
| `copilotcli` | 经 `copilot` CLI 使用 GitHub Copilot | GitHub OAuth 令牌 |
| `custom` | 任何原生讲 OpenAI、Claude 或 Gemini 的 API-key 端点 | `{"api_key"}` |
| `dashscope` | 阿里 DashScope：OpenAI 模式、Anthropic 模式、rerank 与原生多模态图像 API | `{"api_key"}` |
| `deepseek` | DeepSeek：`/v1` 下的 Chat、根路径的 Responses、`/anthropic` 下的 Claude Messages | `{"api_key"}` |
| `glm` | GLM 按量 API；ZCode 官方动态模型目录 | `{"api_key"}` |
| `glmcode` | GLM Coding Plan；独立模型目录、Chat／Messages／Responses、订阅额度查询 | `{"api_key"}` |
| `minimax` | MiniMax 文本 API／Token Plan，以及 H3 视频创建、查询、列表、取消／删除、下载 | `{"api_key"}` |
| `devin` | Devin（Windsurf），`server.codeium.com`：protobuf 上的 Connect-RPC | 会话令牌 |
| `geminicli` | 经 Gemini CLI 所用的 Code Assist 端点访问 Google 账号 | OAuth |
| `grokbuild` | 经 Grok Build CLI 使用 xAI 账号 | OAuth |
| `kimi` | Moonshot 平台（API key），或 Kimi Code 订阅（设备登录） | `{"api_key"}` 或 OAuth |
| `kiro` | 经 Kiro 桌面应用使用 AWS CodeWhisperer | OAuth |
| `nvidia` | NVIDIA NIM：Chat Completions、模型列表与 embeddings | `{"api_key"}` |
| `openai` | OpenAI 官方平台：完整 surface、套接字上的 Responses 与 Realtime | `{"api_key", "quota_api_key"}` |
| `opencodego` | OpenCode Go：订阅制、开源模型、用量窗口 | `{"api_key"}` |
| `opencodezen` | OpenCode Zen：从 Console 余额按量付费 | `{"api_key"}` 或 OAuth |
| `openrouter` | OpenRouter：body 里的路由偏好、应答里报出的价格 | `{"api_key"}` |
| `vercel` | Vercel AI Gateway：模型调用与团队余额查询 | `{"api_key"}` |
| `vertex` | Google Vertex AI：Google、Anthropic 与 OpenAI 兼容发布者 | 服务账号密钥 |
| `vertexexpress` | Vertex AI Express：单一全球源上的 Gemini surface | `{"api_key"}` |
| `workbuddy` | 经编辑器插件使用腾讯 Copilot | OAuth |
| `xai` | xAI（Grok）：OpenAI Chat 与 Responses，加上 xAI 自己的 TTS／STT／视频 | `{"api_key"}` |

每个渠道都能为原生目标**和** `wasm32-unknown-unknown` 构建，所以 Worker 部署不是一个
缩水的渠道集。

### 接入兼容服务

兼容现有协议的服务可以通过 `custom` 接入，填写服务地址和支持的协议即可：

```json
{ "name": "example-vendor", "channel": "custom",
  "baseUrl": "https://api.example-vendor.com",
  "config": { "dialects": ["openai_chat"] } }
```

NVIDIA NIM、Vercel AI Gateway 和 Cloudflare AI Gateway 提供专用渠道，优先使用对应渠道以获得专用的认证和额度查询功能。

## 供应商配置

| 字段 | 含义 |
| --- | --- |
| `name` | 运维者取的标签，唯一。它同时是 **Provider 挂载点**和 `provider/model` 形式的左半边。 |
| `channel` | 上表中的一个 id。 |
| `baseUrl` | 渠道接受 base URL 时的源。 |
| `config` | 该渠道自己的 JSON。每个渠道声明它解码哪些键，`GET /admin/api/channels` 就是那份清单。 |
| `connectionProfileId` | 这个 Provider 的调用走哪套出站栈——代理、TLS 模拟。 |
| `enabled` | 被禁用的 Provider 退出解析。 |

`custom` 的 `config` 带 `dialects`（端点讲哪些协议格式）、静态 `headers`、`allowed_headers`
和两个 magic cache 开关。模拟厂商 CLI 的渠道声明自己的身份 header，并且不让客户端伪造它们。

### Claude 模型 fallback

`fallback_mode` 取 `off`（缺省）、`default` 或 `models`，`fallback_models` 是按顺序排列的 upstream
模型 ID 列表。控制台按渠道描述渲染这些字段。列表会跳过空值、重复项和主模型，最多使用三个 fallback 模型。

- **Claude API、Claude Code 与自定义 Claude 端点：** 发送 Anthropic 的 `fallbacks` 和对应的 beta 头。
  `default` 模式交给 Anthropic 决定。
- **OpenRouter：** Claude Messages 发送 `fallbacks`，Chat Completions 发送 `models`，由 OpenRouter
  执行模型 fallback。这不改变 `provider.allow_fallbacks`，后者控制的是同一模型的供应商路由。客户端
  已带的 `fallbacks` 或 `models` 优先。Responses 请求不会被加上未公开的 fallback 参数。
- **Vercel、Azure、Vertex 与 AWS Bedrock：** GProxy 在同一凭证上用下一个配置的模型重试 Claude
  `refusal`，渠道准备与签名都重新做。`default` 模式选主模型命名空间下的 `claude-opus-4-8`。云厂商
  专用的 ID 请配置该上游接受的完整 ID。

网关 fallback 在协议转换到 Claude 之后同样生效，但不会重试任意 HTTP 错误。流式内容即时下发；已有输出后，
续接需要带 prefill 声明的上游 credit。没有可兑现的 credit 时绝不重放服务端工具。每次物理尝试单独观测、
按其模型计价；报告零输出的 refusal 不计费。最终响应保留最后一次尝试的顶层 usage。

### Claude Code 低优先级模式

`claudecode` 设置 `low_priority: true` 后，每个 Messages 请求都按 CLI 接受低优先级提议后的方式发送
（`anthropic-usage-limit: slow`），5h 窗口用满也不再把凭证移出轮换；周窗口照常封。是否以低优先级
服务由 Anthropic 按账号决定；槽位繁忙时返回 429，GProxy 换下一个凭证。

### 按操作覆盖 URL

`operation_endpoints` 对某个 Provider 的某个 `(操作, 方言, 传输)` 整体替换方法 URL。它
**不是**一个再拼默认路径的 base URL；渠道自己的路径参数由那个方法解析：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/operation-endpoints \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content","dialect":"openai_chat",
       "url":"https://elsewhere.example/v1/chat/completions"}'
```

URL 里的 `{model}` 会被替换成 upstream 模型名（按一个路径段做百分号编码），用于把模型写在路径里的接口：
`https://relay.example/v1beta/models/{model}:generateContent`。

`operation_rules` 是另一半：对某个操作上渠道行为的按 Provider 覆盖。渠道默认值留在代码里，
不会被拷进每个新建的 Provider。
`POST /admin/api/providers/{id}/routing-defaults/reset` 在一次提交里把两者都清掉。

## 一行凭证

| 字段 | 含义 |
| --- | --- |
| `label` | 可选。登录创建时会从渠道和账号推导一个。 |
| `authKind` | `api_key`、`oauth` 或 `cookie`——密钥是怎么得到的。 |
| `secret` | 该渠道声明的字段。从不返回；列表里只有 `hasSecret`。 |
| `organizationId` / `teamId` / `userId` | 归属。恰好一个，或都没有（共享凭证）。 |
| `connectionProfileId` | 覆盖 Provider 的出站栈。 |
| `expiresAtMs` | 密钥该刷新的时间。 |
| `status` | `active` 或 `dead`。死掉的凭证退出计划。 |
| `version` | 刷新写回时所用的 CAS 守卫。 |

密钥用主密钥以 AES-256-GCM 密封，而且**密封绑定到该行自己的 id**，所以一份密文被拷到
另一行上打不开。读回它是一次单独的、被审计的调用：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/reveal \
  -H "Authorization: Bearer $GPROXY_KEY"
```

没有主密钥时密钥以**明文**存放。这是一种受支持的部署，二进制在启动时会说一次。

### 谁能触达它

一把凭证对某个调用方可见，当它**无归属**，或者它的归属与调用方所用 key 的绑定一致——
key 自己的用户、团队，或它的有效组织。

是 *key 的*绑定而不是持有人的成员关系，因为一个用户可以属于两个组织而一把 key 只属于
一个。如果由成员关系决定，一个多组织用户的每把 key 都会触达每个组织的订阅，而且没有一把
key 能比它的持有人更窄。

可见性只收窄凭证集合，从不扩大它。

## 通过登录获取凭证

持有账号而非 key 的渠道提供三种流程中的一种或多种。
`GET /admin/api/channels` 报告每个渠道的 `loginModes`，要一个它不提供的方式会被拒绝。

| 流程 | 步骤 | 适用 |
| --- | --- | --- |
| 授权码 | start → complete | 带 PKCE 的浏览器跳转 |
| 设备码 | start → 反复 poll | 在另一台设备上敲码 |
| Cookie 交换 | exchange | 用户已经有的会话 cookie |

在控制台打开供应商的凭证标签，选择“登录添加凭证”。浏览器授权使用完整回调 URL 完成，
设备码流程由页面自动轮询。五个 POST 操作位于 `/admin/api/credential-login`，待完成会话绑定
发起用户和管理范围。参见[控制台管理](/zh-cn/guides/console/#凭证与上游登录)。

GPROXY 拥有一切不属于上游的部分。**PKCE verifier** 是本地生成的 32 个随机字节，永不外发，
只有它的 S256 摘要会到达授权 URL，因此被截获的授权码没有它也没用。**CSRF state** 本地
生成、本地比对，不匹配不仅拒绝，还顺手销毁会话，重放因此没有第二次机会。

待完成的会话存在**共享 cache** 里，不在本进程。这正是 A 实例开始的登录 B 实例能收尾的
原因——负载均衡后面收尾的通常就是另一个实例——也是被放弃的登录不留痕迹的原因：key 自己
过期，而过期的 key 与从未签发过的 id 无从区分。

**轮询是调用方的事。** 一次设备轮询只走一步就返回，这里既不 sleep 也不循环。应答带着
该等多久，上游的 `slow_down` 会改写这个间隔并对之后每次轮询生效。

一次成功的登录就是一次普通的凭证插入，走管理写入用的同一个提交原语。密钥在语句构造
**之前**就密封好，因此渠道与数据库之间没有任何东西见过明文，返回的只有凭证 id。

### 刷新

持有 OAuth 令牌的渠道声明密钥何时到期，刷新返回的是一份**完整替换**——绝不是合并。宿主
以 `version` 上的 CAS 写回并发出凭证变更通知。

```sh
curl -s -X POST 'http://127.0.0.1:8787/admin/api/credentials/{id}/refresh?force=true' \
  -H "Authorization: Bearer $GPROXY_KEY"
```

上游*确定性*拒绝凭证（`invalid_grant`、已撤销）会把它标记为死。短暂的传输失败或 5xx
绝不能：它必须以传输错误的形式浮现，好让宿主稍后重试。

这条路径绝不能丢写入。Claude 每次刷新都轮换 refresh token，这正是写回是带版本守卫的 CAS
而不是尽力而为更新的原因。

## 健康、封禁与计划

解析直接去掉禁用、已退役和已死的凭证。**被封禁**的凭证——被上游限流的那种——在该
Provider 还有别的可用凭证时被去掉；若**全部**被封禁则保留该 Provider 并排在所有健康
Provider 之后。限流是最后手段，不是故障。

```sh
# 上游怎么说这把凭证的窗口
curl -s http://127.0.0.1:8787/admin/api/credentials/{id}/quota -H "Authorization: Bearer $GPROXY_KEY"
# 现在就问上游
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/quota-probe -H "Authorization: Bearer $GPROXY_KEY"
# 兑换一次重置额度（厂商卖这个的话）
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/quota-reset -H "Authorization: Bearer $GPROXY_KEY"
# 忘掉记录的健康状态
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/health-reset -H "Authorization: Bearer $GPROXY_KEY"
# 覆盖它的运维限额
curl -s http://127.0.0.1:8787/admin/api/credentials/{id}/limits -H "Authorization: Bearer $GPROXY_KEY"
```

`POST …/status` 手动设为 `active` 或 `dead`，带一个原因。

## 连接配置

一份连接配置就是一次调用走的出站栈：代理，以及它呈现的 TLS 与 HTTP/2 身份。选择顺序是
凭证优先，然后 Provider，然后渠道自己的默认，最后实例默认。

内置六种身份预设，可直接存成一份连接配置的 `emulation`：

```sh
curl -s http://127.0.0.1:8787/admin/api/tls-presets -H "Authorization: Bearer $GPROXY_KEY"
```

```text
claude / Claude CLI    codex / Codex CLI       gemini / Gemini CLI
antigravity            kiro / Kiro CLI         copilot / GitHub Copilot CLI
```

只有 `wreq` 传输会呈现指纹。模拟 CLI 的渠道返回自己的默认；凭证或 Provider 上的显式配置
依然胜出。

## 两个探针

这是本层唯一会离开进程的管理调用。

**连通性**通过 scope 指定的那条 client 链，问 Cloudflare 的 trace 端点这套部署从外面看是
什么样：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/connectivity/test \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"scope":"global"}'
```

```json
{"ok":true,"latencyMs":803,"ip":"223.166.167.192","colo":"LAX","error":null}
```

scope 可以是 `global`、`{"scope":"provider","provider_id":"…"}`、
`{"scope":"credential","credential_id":"…"}`——它的调用所用的那条传输——或
`{"scope":"proxy","url":"…"}`，用来测一个还没配置到任何地方的代理。
**网络失败是 `ok: false` 加一个原因，不是错误**："上游不可达"正是问题要的答案。

**模型测试与发现**像调用方的请求一样走完引擎：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/models/test \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","model":"gpt-4o-mini"}'

curl -s -X POST http://127.0.0.1:8787/admin/api/models/discover \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…"}'
```

它们**花的是真凭证、消耗真上游配额、对该凭证适用的任何预算真结算，并写一行用量**。
没有 dry run：一个没有真正调用上游的测试什么也没测到。

发现用 Provider 自己的方言提问，因此名字就是上游的。每一个回来时都带着"这个 Provider
是否已有这一行"和"内置目录能不能给它定价"；
`POST /admin/api/models/discover/apply` 插入你点名的那些，已有的跳过。

## 搬运一份配置

```sh
gproxy export --out config.json --include-secrets
gproxy import --in  config.json --mode merge --source-master-key '…'
```

会走的是一套部署*本身*的配置：连接配置、Provider、凭证、模型目录、路由、操作覆盖、
改写规则、配额、价格和 settings 行。**身份不走**——用户、key、组织、团队、权限和订阅属于
产品层——用量和 capture 也不走，因为拷贝它们等于伪造目的端从未有过的历史。主密钥规则见
[配置](/zh-cn/reference/configuration/#搬运一份配置)。

## 请求头白名单

在“设置 → 网络”配置全局请求头白名单，每个供应商通过 `config.allowed_headers`
补充允许项。最终集合为 **全局白名单 ∪ 供应商白名单 ∪ 渠道默认头**，并保留
`content-type`。未配置或空列表不增加允许项，其他客户端头默认丢弃。原始鉴权头和
逐跳头即使列入名单也会移除；渠道注入的鉴权和静态 `headers` 配置独立于此规则。
配置热更新后生效，不会把合并结果写回供应商保存的列表。

### GLM 与 MiniMax

`glm` 使用按量 API，`glmcode` 使用 GLM Coding Plan，两者都填 `api_key`。默认地址是国内 `https://open.bigmodel.cn`；国际账号将 `baseUrl` 设为 `https://api.z.ai`。`glmcode` 支持官方 Chat、Claude Messages 和 Responses 订阅入口，不会在额度耗尽后切换到普通 API。

两个渠道的模型列表均沿用 ZCode 官方动态目录：查询 `/api/v1/client/configs`，下载返回的 `builtin_provider_config_json`，再选择对应地区的普通 API 或 Coding Plan 模板。没有内置固定模型名单，也不在下载失败时回退到旧名单。目录是官方产品列表，不代表该 Key 对每个模型都有权限。

`minimax` 默认地址为 `https://api.minimax.io`；国内账号按其平台文档填写国内 API origin。文本支持 OpenAI Chat 和 Claude Messages，Subscription Key（Coding Plan／Token Plan）与按量 API Key 都填入 `api_key`。模型列表和详情使用官方 `/v1/models` 与 `/v1/models/{id}`。两类 Key 可以放在不同供应商中，分别配置文本与视频模型路由。

额度查询：`glmcode` 读取官方 `/api/monitor/usage/quota/limit`，显示服务端提供的用量百分比、独立窗口和重置时间；普通 `glm` 尚未接入账户余额查询。MiniMax 按官方 CLI 的 Key 类型判断读取 `/v1/token_plan/remains` 或 `/account/query_balance`。当前这些查询结果用于展示，是否耗尽仍由上游请求结果判定。

MiniMax 视频接入 **H3 V2**（`MiniMax-H3`、`MiniMax-H3-Max`），需要按量 API Key。使用 GPROXY 的 OpenAI 视频扩展 JSON：

```json
{
  "model": "MiniMax-H3",
  "prompt": "一只猫在花园里奔跑",
  "duration": 5,
  "resolution": "2K",
  "aspect_ratio": "16:9"
}
```

向 `/v1/videos` 提交后，使用返回的 `id` 查询 `/v1/videos/{id}`，完成后从 `/v1/videos/{id}/content` 下载。支持 `frame_images` 首尾帧及 `input_references` 图像、视频、音频参考，也可传 MiniMax 原生 `content`。不支持 Sora multipart、`seconds` 或 `size` 参数；旧版 Hailuo 2.x V1 接口不在本渠道覆盖范围内。

列表使用 `page_num`／`page_size`（`limit` 会映射成 `page_size`），不支持游标 `after`／`order`。删除接口保留上游的 `action`：`cancelled` 表示取消待执行任务，`deleted` 才表示删除任务记录。渠道不会自动轮询，也不会保存视频文件。

接口依据：[ZCode 动态目录实现](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/packages/provider-node/src/zcode-builtin-download.ts)、[MiniMax 官方 CLI](https://github.com/MiniMax-AI/cli)、[GLM Coding Plan](https://docs.bigmodel.cn/cn/coding-plan/tool/others)、[MiniMax 文本](https://platform.minimax.io/docs/api-reference/text-anthropic-api)、[MiniMax H3 V2](https://platform.minimax.io/docs/api-reference/video-generation-v2-create)。
