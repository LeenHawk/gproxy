---
title: "嵌入核心库"
description: "gproxy-sdk 是可嵌入的句柄：装配、管理写入、登录、解析、调用、查询与同步——里面没有 server，也没有身份。"
---

`gproxy-sdk` 是可嵌入的 GPROXY 句柄。网关本身就建在它上面，而一个应用可以直接链接它。

**它卖的不是 HTTP 客户端，而是一次带池化凭证纪律的调用**：凭证池、自动刷新、Provider 之间
的失败转移，以及被计量的结算。这也是为什么数据库在它内部不能省掉——Claude 每次刷新都轮换
refresh token，一个把凭证放内存里的句柄会在第一次刷新就把它弄死。

workspace 里没有任何东西发布到 registry。嵌入意味着一个 git 或路径依赖。

```toml
[dependencies]
gproxy-sdk = { git = "https://github.com/LeenHawk/gproxy", branch = "4.0" }
```

workspace 是 Rust edition 2024，版本 `4.0.0-dev`。**公开接口尚不稳定。**

## 装配一个

```rust
use gproxy_sdk::{GproxyBuilder, SyncMode};

let gproxy = GproxyBuilder::sqlite("gproxy.db")
    .await?
    .master_key([0u8; 32])
    .sync_mode(SyncMode::Background)
    .build()
    .await?;

for channel in gproxy.channels() {
    println!("{} ({})", channel.display_name, channel.id);
}
println!("serving revision {}", gproxy.revision().0);
```

`build()` 不会打开任何没交给它的东西。它同步实体表结构（除非被告知不要）、创建 settings
行、装配引擎、加载第一份快照，并在后台模式下启动订阅与轮询循环。

**密钥编解码器绝不隐式选择。** `master_key` 用 AES-256-GCM 密封，`plaintext_secrets` 是
显式的反向选择，两者都没有的 build 会被拒绝。不存在一个安静的默认值，事后才发现它一直是
明文。

## Feature

| Feature | 作用 |
| --- | --- |
| 某个渠道的名字（`codex`、`kiro`、`openai`…） | 把那个渠道编译进来并注册 |
| `channels` | 本 workspace 提供的每个渠道 |
| `postgres` / `mysql` | 额外驱动，仅原生。SQLite 在原生上始终可用 |
| `libsql` | 经 Hrana HTTP pipeline 的 libSQL/Turso，**每个目标** |
| `d1` | Cloudflare D1 binding 的标记 |
| `memory`（默认） | 进程内 cache，原生上的默认 |
| `redis` | 多实例用 |
| `fs`（默认） | 本地文件存储，仅原生 |
| `s3` | S3/R2 存储 |
| `bundled-vocabulary`（默认） | 随包的 DeepSeek 词表，用于 token 估算 |
| `ts` | 为每个 DTO 和每个渠道 descriptor 生成 `ts-rs` 声明 |

默认集合能为 `wasm32-unknown-unknown` 构建，`libsql` 也能。

原生构建使用 `reqwest` 传输。想要别的后端的宿主——`wreq` 做 TLS 模拟、原生栈匹配 Codex
CLI 的指纹——在 client crate 上打开那个 feature，或者把自己的 client 池交给 builder。

## 调用

```rust
let execution = gproxy
    .call(
        OperationKey { operation: Operation::GenerateContent, dialect: Dialect::OpenAi },
        request,
    )
    .scope("user:u-1")
    .attribution(UsageAttribution { user_id: Some("u-1".into()), ..Default::default() })
    .budgets(vec![BudgetOwner::new("user", "u-1")])
    .credentials(["cred-1".to_owned()].into())
    .send()
    .await?;

let (response, usage) = execution.into_parts();
```

`call` 组装请求；`send` 解析模型名并走完计划。

**scope 必填，而且它没有安全的默认值。** 它是引擎把一个调用方的凭证亲和性关在里面的隔离
边界，一个默认值就意味着陌生人共享续接。

`connect` 是同一个 builder 在 WebSocket 握手上的版本。

`.channel("claudecode")` 收窄候选集。那是收窄，不是选 Provider：渠道对调用方有实质差别
——`claudecode` 花的是 OAuth 订阅额度，`claudeapi` 花的是按 token 计费的 API key，配额池和
计费都独立——而 Provider 只是一行实例外的人不该需要知道的配置。钉了渠道之后句柄的活还很多：
在该渠道的多个 Provider 里选、在一个 Provider 的多把凭证里选，并在一把失败时在渠道内转移。

应用层决定的一切——允许的 Provider 与凭证、预算链、会话——都是**传进来的**。这里不认证任何人。

## 这里没有什么

- **身份。** 用户、API key、组织、团队、权限、订阅、限流和 OAuth issuer 属于 `gproxy-app`，
  它从同一个批里读同一个持久 revision。
- **下游认证。** 这里没有任何东西决定调用方是谁。
- **server。** 没有监听器、没有 router、没有中间件、没有 CLI。
- **console。** 这个 crate 生成 UI 所依据的 TypeScript 类型；UI 本身是应用的。
- **执行。** 尝试、转换、改写、观测、预算与结算属于引擎；这个 crate 只决定交给它什么。

## 管理

`gproxy.manage()` 是写侧：每个配置家族一个访问器，全部走同一个提交原语。

| 家族 | 表 | CRUD 之外 |
| --- | --- | --- |
| `providers()` | `providers` | `reset_routing_defaults` |
| `credentials()` | `credentials` | `reveal_secret`、`set_status`、`refresh`、`quota_probe`、`quota_read`、`quota_reset`、`health_reset`、`limit_status` |
| `models()` / `provider_models()` | 目录 | |
| `routes()` / `route_members()` | 路由 | |
| `connection_profiles()` | 出站栈 | |
| `settings()` | 那一行 settings | 只有 `get` / `update` |
| `rewrite()` | 规则集、规则、挂载 | `replace_rules` |
| `endpoints()` | `operation_rules`、`operation_endpoints` | |
| `quotas()` | `quotas` | `budget_status`、`reset_budget`、`limit_status`、`reset_limit` |
| `pricing()` | 规则、费率、档位 | |
| `transfer()` | 全部配置 | `export`、`import` |
| `catalog()` | 应用前什么都不写 | `channels`、`default_models`、`apply_default_prices`、`tls_presets`、`rule_presets` |
| `connectivity()` | `provider_models` | `test`、`model_test`、`discover_models`、`apply_discovered` |
| `tokenizer()` | 词表文件 | `vocabularies`、`fetch`、`progress`、`delete`、`auth` |

前十个是普通 CRUD 加一个批量。id 给了就用、没给就生成，时间戳是 Unix 毫秒，小数以字符串
传输，而凭证的密钥从不出现在任何 DTO 里。

一次写入声明自己的 scope，这是调用方唯一的选择。凭证状态 scope 走便宜的重载路径，并且
**仅**适用于只改密钥、过期时间与生命周期状态的写入——凭证的其余部分在装配时就被冻进执行
快照，改了就必须整体重载。

## 查询

`gproxy.query()` 是读侧。这里不写任何东西，因此也不动 revision。

| 家族 | 回答什么 |
| --- | --- |
| `usage()` | `records`、`summary`、`group`、`trend` |
| `quota()` | `windows`、`settlements`、`credential_cycles`、`counted_windows`、`budget_status` |
| `logs()` | `list`、`detail(request_id)` |

**聚合在 Rust 侧完成，并带扫描上限。** 用量的 metrics 列是每请求一份 JSON 文档，本产品
支持的任何后端都无法在其内部求和，因此 `summary`、`group` 和 `trend` 读出匹配行并按键序
分块折叠。每个聚合都有一个扫描预算，默认并被钳制为 50 000 行，而用满预算的聚合会带着
`truncated: true` 和扫描计数回来——绝不把一个更小的数字当成全部事实。趋势还有第二重边界：
零或负的桶宽、倒着的区间，以及会产生超过 5 000 个桶的区间，一律拒绝。

有两个数不来自那份文档。费用读的是结算时写一次的那个带索引的列；而当被扫到的记录在币种
上不一致时 currency 是 `None`，因为把美元和欧元加起来不是一个合计。

**管理列表用 offset 分页，请求日志用游标。** 这不是风格选择：管理列表是人翻的有界集合，
而请求日志是一边被读一边在增长的追加流，offset 在它上面会重复或漏行。日志游标是两半的
——一个时间戳加行 id——因为两个请求可能起始于同一毫秒，只有时间戳的游标要么在那一对上
永远打转，要么直接跳过它。

**`logs()` 里什么都不脱敏。** 观察者在*写*这些行时就应用了部署的策略，所以存下来的已经是
可以展示的。宿主不能假设读的时候还有第二趟：如果密钥在数据库里，就是策略允许它在那里，
而读时过滤撤销不了这件事。

## 同步

| | 传递什么 | 会怎么失败 |
| --- | --- | --- |
| 共享 cache 上的失效通知 | 毫秒级的"再看一眼" | 丢消息 |
| `settings.config_revision` 轮询 | 持久事实，默认 30 秒 | 慢 |

wasm 上 cache 默认是数据库承载的那一个，而同步一律是**手动**的：isolate 活不过一次请求，
没有后台循环可跑，所以请求开头的 `tick()` 就是全部机制。

```rust
// tick() = 取走已投递的通知，然后轮询一次 revision。
instance.tick().await;
```

取通知便宜到可以每请求都做，轮询不是，因此只想要便宜那一半的宿主可以单独调它。手动模式
在 build 时就建立订阅并丢掉队首的 resync 提示——首次装载*就是*那次 resync——否则介于装配
与第一次 tick 之间的写入会被两套机制同时漏掉。

## TypeScript 导出

```sh
GPROXY_TS_OUT=console/src/generated \
  cargo test -p gproxy-sdk --features ts export_types
```

没有这个环境变量时测试直接返回、什么也不写，所以 `cargo test --all-features` 保持无副作用，
而生成目录只会被有意地重写。有它时，目录先被**清空**——一个已经不存在的 DTO 留下的陈旧
声明，会在 Rust 早就删掉它之后还让类型检查通过——最后写一个把一切 re-export 出去的索引。

渠道目录也在里面。console 从渠道自己返回的 descriptor 渲染 Provider 表单，所以它应该被
那份 descriptor 定型，而不是被一份会在渠道下次新增配置键时漂走的副本。

导出清单是手写的，因为 Rust 没有运行期枚举模块类型的办法。那份清单正是 v3 漂走的东西
——加了一个 DTO，没人记得加进清单，于是 console 悄悄地没有了它的类型——所以第二个测试读
模块自己的源码，把它的 re-export 与清单对比。**给 dto 加一个类型却忘了加进清单，是一个
红色的测试，而不是一个缺失的文件。**

## SDK 之上

同时想要身份的应用链接 `gproxy-app`，它补上认证、准入、管理面与用户面的操作家族以及
OAuth issuer，而且**不含**任何传输层。`gproxy-host-axum` 把它们变成一个 HTTP surface，
`gproxy-host-edge` 在 Worker 里挂载同一个 router，`gproxy-host-tauri` 把管理面绑到 IPC。

其中每一道接缝都是嵌入者可以进入的地方。见[架构](/zh-cn/introduction/architecture/)。
