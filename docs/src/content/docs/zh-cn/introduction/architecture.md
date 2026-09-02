---
title: 架构
description: GPROXY v3 的构成 — 一个可嵌入的核心、可互换的宿主、唯一的结算漏斗、成对的协议转换，以及单一的 schema 定义。
---

GPROXY v3 是 v2 网关的 Rust 从零重写。它做的事没有变：在众多 LLM
Provider 之前提供一个 API 密钥入口，池化上游凭证，负责路由与故障转移、配额准入、
用量核算与计费，在不同线协议之间转换，并模拟厂商控制平面，让官方 CLI 使用池化凭证运行。
变的是结构。在 v2 中，接入一个新功能最多要改 8 层共 63 个文件，每个功能 diff
有一半是接线代码。v3 的设计目标是：一个功能只落在它的事实所在之处。

本页是地图。它描述各个 crate、核心与宿主之间的边界、执行模型，以及防止结构回退的规则。

## 一个核心，多个宿主

`gproxy-core` 是一个可嵌入的库：通道、凭证生命周期、协议转换与执行流水线。宿主把它适配到某个运行时：

| 宿主 | 运行时 | 说明 |
| --- | --- | --- |
| `gproxy-host-axum` | 原生二进制（`gproxy`） | Tokio + axum 监听器，内嵌控制台，公告、自启动、自更新。 |
| `gproxy-host-edge` | 基于 fetch 的平台 | Cloudflare Workers、Deno Deploy、Netlify Edge；编译到 `wasm32-unknown-unknown`。 |
| 你的应用 | 直接嵌入 | 链接 `gproxy-core`（或 `gproxy-app`），调用同一个执行入口。 |

每个宿主调用同一套公开 API。网关自己的 HTTP 层没有进入引擎的私有入口：v2 的
Codex 服务面是一段 1,700 行、与流水线并行的重新实现，把真实推理直接转发上游而不做结算，
这段未计费流量直到 v2.9.1 才被发现。v3 中这类 bug 写不出来，因为核心是通往上游的唯一路径。

## Crate 依赖图

```text
   gproxy-host-axum        gproxy-host-edge         你的应用
   原生监听器               fetch 运行时              直接嵌入
          \                     /                          |
           +-----> gproxy-app <+                           |
                   启动 · 配置 · 快照 · 装配                 |
             /       |         |          \                |
  gproxy-admin  gproxy-store  gproxy-upstream  gproxy-tokenize
  DTO (ts-rs)   4 种 SQL 方言  HTTP/WS 客户端   token 计数
  纯分发        + 缓存后端      TLS 指纹
             \       |         /                           |
              v      v        v                            v
              gproxy-core  <-------------------------------+
              执行引擎 · 宿主 trait · 边界类型
                 |                    |
          gproxy-channels       gproxy-transform
          28 个上游适配器        成对转换 + 封装适配器
                 |                    |
          gproxy-channel-api          |
          通道契约                     |
                 \                   /
                  gproxy-protocol
                  线协议模型 · 操作分类 · OperationSpec
```

箭头只向下。宿主依赖核心，反向永远不成立；核心永远不会获得监听器、路由器、UI
或具体的存储实现。`gproxy-app` 是第一个嵌入者：它把存储后端、管理平面、上游传输和分词器装配在核心周围，
宿主再把 `gproxy-app` 适配到运行时。另一个嵌入者可以只链接 `gproxy-core`，自己提供宿主 trait。

| Crate | 职责 |
| --- | --- |
| `gproxy-protocol` | OpenAI、Claude、Gemini 的类型化线协议模型；操作分类；`OperationSpec`，每个操作的唯一声明。 |
| `gproxy-channel-api` | `Channel` 契约：描述符、能力行、路由默认值、请求准备、流解码、用量提取、登录与刷新、服务面表、客户端指纹数据。 |
| `gproxy-transform` | 线协议之间成对的请求、响应与流转换，以及封装适配器。 |
| `gproxy-channels` | 内置适配器，每个通道一个目录。 |
| `gproxy-core` | 引擎：分类、认证、解析、准入、转换槽位、故障转移、结算漏斗、服务面；宿主 trait；不依赖框架的边界类型。 |
| `gproxy-upstream` | 标准的 `UpstreamTransport`：带 TLS 指纹配置的原生 HTTP/WebSocket 客户端，以及面向 wasm 的 fetch 客户端。 |
| `gproxy-store` | 面向 SQLite、libSQL、PostgreSQL、MySQL 的单一 schema 目录与单一查询层；缓存后端。 |
| `gproxy-tokenize` | 离线 token 计数：tiktoken 词表、Hugging Face 分词器、内置 DeepSeek 词表，以及字符估算。 |
| `gproxy-admin` | 管理与门户 DTO（用 `ts-rs` 生成 TypeScript）以及纯函数式的 `(state, request) → response` 分发。 |
| `gproxy-app` | 启动：配置、存储与缓存选择、控制平面快照、通道注册、`App::start`。 |
| `gproxy-host-axum`、`gproxy-host-edge` | 运行时适配器。 |

## 边界

核心使用 `http` crate 的类型，再加上少量自有类型。宿主从原生请求类型构造它们，再渲染回去：

- `RequestCtx` — 方法、路径、查询、头部、以引用计数 `Bytes` 承载的请求体、WebSocket
  升级状态、路由模式、请求 id。
- `ExecOutcome` — 状态码、头部、请求体（完整字节、`ByteStream` 或 WebSocket 双工之一）以及处置结果。
- `ByteStream` — 两个方向共用的唯一流类型。零拷贝透传是默认路径；只有在转换必须改写时才改写流。
- `CoreError` — 分类为认证、路由、上游、传输或内部错误，附带不依赖框架的渲染辅助。框架响应转换只存在于宿主，绝不在核心。

宿主服务通过 trait 进入：

| Trait | 宿主提供什么 |
| --- | --- |
| `CredentialStore` | 必需。加载密钥，并在版本守卫的 compare-and-swap 之后原子地持久化轮换后的令牌。Claude 每次刷新都会轮换 refresh token；没有原子持久化的嵌入者会在核心第一次刷新时弄坏自己的凭证。 |
| `CacheBackend` | 带 TTL 的共享缓存，用于配额预留、限流计数、刷新租约、准入状态和登录会话。单实例用进程内实现；多实例用 Redis、Upstash 或 libSQL。 |
| `UpstreamTransport` | 出站 HTTP 与 WebSocket。trait 定义在核心里，核心因此不依赖具体客户端。 |
| `UsageSink`、`CaptureSink` | 漏斗的输出：结算记录与线捕获。应用写入数据库行；嵌入者可以聚合或丢弃。 |
| `Spawner` | 可选。存在时，流的结算在最后一个字节之后脱离请求（原生服务器）；缺失时，结算在流关闭前内联完成（edge）。策略*就是*该能力是否存在；没有策略枚举，也没有 `#[cfg]` 分叉。 |
| `BindingStore` | 按 Provider 和所有者划定范围的持久资源绑定（文件、视频、任务），让池化的后续请求到达创建该资源的凭证。 |
| `ControlPlane` | 引擎解析所依据的 Provider、路由、密钥与设置的窄只读视图。应用基于其快照实现它。 |

解密是存储实现者的事。`gproxy-app` 在存储内部做信封加密；简单的嵌入者可以明文存储；核心永远看不到密文。

## 执行模型

引擎是一段固定顺序、可组合的阶段序列。每条路径都是同一组阶段的组合，最后三个阶段不可省略：

```text
ingress → classify → authenticate → resolve(route | named target)
        → admit(权限 · 限流 · 配额预扣)
        → transform? → 请求规则 → channel.prepare → transport
        → 响应规则 → 反向 transform? → disposition
        ↺ 在预算内围绕 prepare / transport / disposition 故障转移
        → settle → capture → telemetry            ← 漏斗：始终运行
```

两个公开层级包裹着它：

- **第一层 `invoke`** — 一个凭证、一个已知线协议形状的请求、不做路由。分类、准备、发送、结算。这是嵌入者的
  SDK，带有池化凭证的纪律。
- **第二层 `execute`** — 完整序列：多凭证故障转移、转换、亲和。网关的数据平面只使用它。

漏斗由类型系统强制。每个 `ExecOutcome` 都携带一个只有漏斗模块才能构造的 `Settled`
证明，因此没有任何代码路径能返回一个跳过结算的响应。跳过漏斗的路径不是快速路径，而是未计量的流量，并且无法编译。

服务面 — Codex CLI、Claude Code 及其他 CLI 通道背后模拟的厂商控制平面 — 是声明出来的，不是临时写的。
通道注册一张路由模式表，每一项映射到本地合成或上游转发。本地合成以零用量通过漏斗退出；转发通过第一层退出。
WebSocket 入口同理：一张已声明升级的注册表，绝不是网关里的 if 链。

## 请求生命周期

```text
客户端请求
  └─ 宿主适配器：原生请求 → RequestCtx
       ├─ /admin/api/**  → gproxy-admin 分发（独立的会话认证、审计日志；
       │                    控制平面，永不触碰用量）
       └─ 数据平面
            ├─ authenticate：API 密钥 → 身份
            ├─ 匹配服务面 / WebSocket 注册表？
            │     ├─ 是 → 服务面准入 → 本地合成或第一层转发
            │     └─ 否 → 第二层
            │              classify (OperationSpec) → 解析路由或命名目标
            │              → admit：权限 · 限流 · 配额预扣
            │              → 入站线协议 ≠ 上游线协议时转换
            │              → channel.prepare（URL · 认证 · 整形）
            │              → transport 发送（零拷贝 ByteStream）
            │              → disposition ──故障转移──▶ 下一个凭证
            └─ 漏斗（始终运行）：结算用量 · 计价 · 对账配额 → UsageSink
                                capture → CaptureSink · telemetry
                                流与 socket 在结束 / 关闭时结算
  └─ 宿主渲染 ExecOutcome（或一次被记录的错误退出：退还预扣、
     捕获、遥测，不做结算）
```

1. 宿主读取原生请求，构造 `RequestCtx`，调用应用。
2. 先检查服务面表。匹配到的项本身就是一组阶段组合。
3. 否则进入第二层：用 `OperationSpec` 分类、认证、解析、准入，入站与上游形状不同时转换，
   应用请求规则，在通道中准备，发送，应用响应规则，反向转换，处置，并在尝试预算内故障转移。
4. 漏斗结算用量、计价、对账配额预留、写入用量行与线捕获、发出遥测。流在结束时结算。
5. 宿主渲染 `ExecOutcome`。

## 操作注册表

一个操作只声明一次。`gproxy-protocol` 里的 `OperationSpec`
为每个操作声明：所属分组、各线协议族的入口路径模式、请求目标、请求体与流的预期、可计费性与结算模式、
亲和类型，以及是否为 WebSocket 升级。分类、通道、路由默认值、结算和控制台生成的元数据都读取这唯一的声明。
在 v2 中，同样的事实散落在十多个 match 点和五份平行的可计费操作列表里，被漏掉的正是第五份。

协议枚举在工作区内是穷尽的。新增一个变体会产生一份编译错误清单，列出每一个需要更新的位置；
不存在那种编译通过、运行时 panic 的 `_ => unreachable!()` 分支。

## 线协议模型与转换

`gproxy-protocol` 是一个类型化的 schema 库，不是内部细节。每个操作都有完整的请求、响应与流事件模型，
并对照上游 API 文档核对。两条规则让它在规范变动下仍可维护：

- **未知字段得以保留。** 每个模型结构体都带有一个扁平化的 `rest` 映射，代理从未见过的字段会原样到达客户端。
  丢弃未建模字段的代理是错的。
- **缺失即含义。** 可选的线协议字段在模型与转换中始终保持可选，序列化时省略。上游没有发送用量时凭空造出
  `usage: {0,0,0}`，既欺骗客户端，也欺骗结算 — 结算必须回退到估算。

转换**按设计成对进行；没有中间表示**。上游规范的变化速度超过任何中枢格式的跟进能力，而转换保真度就是产品本身。
OpenAI Chat Completions、OpenAI Responses、Claude Messages 与 Gemini GenerateContent
之间全部六对双向转换都已存在，含缓冲与流式。既有格式的传输或封装变体 — 走 WebSocket 的线协议、SSE
重新分帧、Gemini CLI 与 Antigravity 共用的 Code Assist 封装 — 叠加在既有的对之上，而不是新开一族。

流式转换是显式的状态机。存在三种分帧：SSE、WebSocket，以及 Gemini 的增量 JSON 数组（不带
`?alt=sse` 的 Gemini 路径默认如此）。分帧是双边线协议契约的一部分：无论上游产出什么，调用方都收到它请求的分帧。
截断或无效的流会传播中断；没有解码器会捏造一个成功的终止符。

Provider 原生工具 — Claude 的 `bash` 与 `text_editor`、Responses 的 `shell` 与
`apply_patch`、Gemini 的 `code_execution` — 映射到目标上最接近的原生工具，近似而非完全相同，
调用与结果的生命周期在转换中保持关联。没有对应物时，工具降级为一个声明的 function tool，而不是被丢弃。

## 通道

通道是上游适配器。`Channel` trait 是同步且对象安全的：准备、分类、流解码和用量提取都是对借用数据的纯逻辑，
注册表因此只需持有普通的 trait 对象。唯一的异步关注点 — OAuth 刷新 — 返回一个装箱的 future，
并在独占的缓存租约下通过宿主的版本守卫存储持久化。

通道以数据形式声明：

- **能力行** — 哪个面向客户端的操作映射到哪个通道原生操作，注册表因此准确知道一个请求需要哪一对转换；
- **路由默认值** — 按操作与入站协议给出透传、转换、本地或不支持，写入每个新建 Provider，并可在重置时重新计算；
- **服务面表**及任何**操作驱动**（Claude Web 这类多步浏览器回合是声明式状态机，其副调用由核心传输并经过漏斗）；
- **客户端指纹** — ALPN、TLS 版本区间、密码套件与曲线列表、HTTP/2 设置、头部顺序 — 全是普通数据。
  传输层只持有一份通用翻译，不认识任何通道名；
- **登录模式** — 带 PKCE 的授权码、设备码或 cookie 交换 — 控制台无需通道特定 UI 即可渲染；
- Provider 与凭证表单所需的**字段**，附带标签与帮助文本，同样以通用方式渲染。

共发布 28 个通道：API 密钥 Provider、使用服务账号或 SigV4 认证的云平台、聚合网关，以及 CLI 模拟
（Codex、Claude Code、Gemini CLI、Copilot CLI、Kiro、Cline、Grok Build、OpenCode、WorkBuddy、
Antigravity）。Claude Web 仅限原生宿主，因为它的实时工具续接是进程本地的。

## 路由与准入

模型预处理按 别名 → 后缀 → 路由 的顺序运行。路由成员携带 `tier`（故障转移层级：tier 0
的全部成员都会先于 tier 1 中的任何成员被尝试）和 `weight`（同一层内的分配比例）。凭证同样带有权重。
选择是对权重展开序列的确定性计数轮转 — 没有随机性，因此 wasm 与原生行为一致，重放可复现。
健康度按凭证与模型跟踪，一个模型上的 429 不会连带拖垮该凭证的其他模型。

一切限制或计量流量的东西都经过 `CacheBackend`：配额预留、限流计数、准入状态、刷新租约。
使用进程内后端时，一个实例就是单实例，这是受支持的默认配置。共享后端是让第二个实例正确运行的前提。

## 规则

操作者无需改代码即可为每个 Provider 配置两层规则。**路由规则**按操作与入站协议决定请求是透传、
转换到某个目标、本地应答，还是不支持。**规则集**承载有序的变更 — 系统文本、缓存断点、JSON
改写、结构变换、头部 — 在转换之后、进入通道之前作用于 Provider 原生请求，并在反向转换之前作用于响应。
流式规则在每个完整帧经过时改写它；没有任何东西会把实时流缓冲到结束。编译进二进制的通道整形是厂商行为；
规则才是操作者在运行时改变的东西。

## 计费与结算

`Settlement` 只有一个构造点。`NormalizedUsage` 保留一等的 token 字段，外加 `metrics`
与 `dimensions` 两个映射。新的用量度量 — 音频秒数、图片、搜索调用、缓存 token —
进入这两个映射，由数据驱动的费率规则计价；这条路径只需改动两处。把某个度量提升为列，只在有证据时进行。

计价在同一组行上有两个轴。一条分层行可以指定 `service_tier`（batch、priority、flex）、
`min_prompt_tokens` 阈值（长上下文阶梯）或两者兼有，携带显式价格或倍率。显式的分层价格替换基础阶梯；
倍率则与之叠加。请求在准入时声明一个层级用于预扣估算；响应报告实际服务的层级，结算按后者收费。

Realtime 通话从服务端计量：代理为返回的 call id 打开 OpenAI 的旁路连接，从 `response.done`
与转写完成事件中读取用量。客户端上报的总量绝不被信任。

## 持久化

单一 schema 目录，单一查询构建层。SeaQuery 为 SQLite、PostgreSQL、MySQL
生成 DDL 与全部业务语句；SQLite 方言同时通过 libSQL 的 Hrana 协议运行，这就是 wasm 宿主的持久化方式。
后端一致性是一项测试 — 比较表与列集合，并在每个后端上跑同一个共享场景 — 而不是靠纪律。
迁移有编号且单调，从第一个版本一路迁移上来的数据库与全新创建的数据库收敛一致。

控制平面读取走一份在写入时重建、原子替换的快照。写入同时会在共享缓存里递增一个失效计数器；
原生实例每秒轮询一次，Edge isolate 每次请求时检查，版本变化后重建自己的快照。

## 管理平面与 Web 界面

`gproxy-admin` 持有 DTO 和一个纯分发函数。每个宿主都为 `/admin/api/**` 与 `/portal/api/**`
调用同一个分发；没有框架特定的管理路由器。管理 DTO 派生出 TypeScript 定义，作为 `cargo test`
的一部分重新生成，控制台导入这些生成文件而不自己声明 — 手写的 Rust 类型镜像即便恰好一致也是 bug。

同一个二进制用一个 React 应用提供三个界面：`/` 的公开产品页、`/admin` 的操作控制台、`/portal`
的用户门户。参见[控制台、门户与公开站点](/zh-cn/guides/console/)。

## 各宿主额外提供什么

- **原生（`gproxy`）** — 带入口大小与并发限制、请求 id、优雅关机的 axum 监听器；一个
  `Spawner`，让流的结算脱离请求；带按通道 TLS 指纹与代理选择的 `wreq` 传输；签名公告源、按用户的自启动，
  以及带签名清单与回滚的自更新。配置来自环境变量与 `.env`，启动时读取一次。
- **Edge** — 由平台绑定组装的类型化配置，Fetch 请求转换为 `RequestCtx`，具备 pull
  与取消排空的流以保证内联结算完成，平台提供升级能力时泵送 WebSocket，否则明确返回 501。libSQL
  是存储；Upstash 是可选的共享缓存。
- **嵌入** — `App::start(config)` 返回一个带 execute、mutate、reload、shutdown
  的句柄；或者只链接 `gproxy-core`，实现宿主 trait，直接调用 `invoke` 或 `execute`。参见
  [嵌入核心库](/zh-cn/reference/embedding/)。

## 让结构保持如此的规则

- 核心永不依赖服务器框架或 UI。
- 每个到达上游的请求都经由同一个漏斗退出。
- 一个操作只在 `OperationSpec` 中声明一次。
- 协议枚举穷尽；清单由编译器给出。
- 转换成对进行；没有中间表示。
- 单一 schema 定义、单一查询层，一致性由测试强制。
- 新的用量度量先作为维度，有证据时才成为列。
- wasm 是一等的核心目标；运行时差异是宿主能力，不是 `#[cfg]` 分叉。
- 请求体以引用计数字节流动，只在需要它的阶段解析一次。
- 前端类型由 Rust 生成，绝不手工编辑。
