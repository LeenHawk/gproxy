---
title: "边缘部署（Cloudflare Workers）"
description: 把 GPROXY v4 作为 Cloudflare Worker 跑在原生二进制服务的同一个 axum router 上：binding、两级配置、它拒绝什么，以及体积约束。
---

`gproxy-host-edge` 是 `gproxy-host-axum` 的 router 之上的一个 Cloudflare Workers `fetch`
handler。

**这个 crate 里没有路由表。** 它挂载的是原生二进制服务的同一个 `axum::Router`，同样的路由。
原生宿主新增的路由就是这个宿主提供的路由，这边一行都不用改。

```rust
#[event(fetch)]
async fn fetch(request: HttpRequest, env: Env, _ctx: Context)
    -> worker::Result<http::Response<axum::body::Body>>
{
    let instance = instance(&env).await?;   // 每个 isolate 装配一次
    instance.tick().await;                  // 追上别的实例
    Ok(instance.router().call(request).await?)
}
```

这就是这个宿主的全部。其余都是装配与配置。

这是 v4 新增的。v3 的 edge 宿主是一份在 `web_sys::Request` 之上手写的分派；v4 把它删了，
因为另一条路——在这个 crate 里把四十来条管理路由再写一遍——有一个引线更长的同样故障：
两份列表会漂开，而且没有人会发现，直到某个 console 按钮只在生产环境上 404。

## 它不提供什么

| Surface | 为什么 |
| --- | --- |
| websocket / realtime | Worker 的升级方式是构造一个 `WebSocketPair` 并把客户端那一半放进响应的 `webSocket` 字段——这个机制**根本没有 `http::Response` 形状**。握手以 `501` 拒绝。 |
| 内嵌 console | 那份 bundle 属于这个 Worker 前面的 Workers Assets。 |

其余全是 axum router 的，原样保留：数据面及其渠道服务路由、OAuth issuer、`/admin/api`、
`/portal/api`、`/publications/{id}` 和 `/healthz`。

把 realtime 做成真的需要两块东西，都不是路由问题：协议套接字类型的一个 `WebSocketPair`
实现，以及一条让 handler 把准备好的 JS 响应交回 fetch 入口的通路。第二块已经有接缝了。
它**刻意没做**：一条从未对着真实客户端跑过的升级路径，比一次诚实的拒绝更不值钱。

## 体积是一个真实约束

在本仓库上用一次普通的 `wasm32-unknown-unknown` release 构建测量，在 `wasm-bindgen` 与
`wasm-opt` **之前**：

| 构建 | 原始 | gzip |
| --- | --- | --- |
| `--features channels`（全部 25 个） | 25.9 MB | 7.7 MB |
| `--features d1,custom,codex,claudecode` | 24.8 MB | 7.4 MB |

Cloudflare 的限制是对**压缩后**的包：免费计划 3 MB，付费 10 MB。`wasm-opt -Oz` 和
`wasm-bindgen` 的垃圾回收能砍掉其中相当一部分，而上面这些数字里没有应用它们。

**没有测量过任何一次部署，因为本仓库没有 Cloudflare 账号也没有 wrangler 工具链。**
部署方必须检查自己的包。

逐个点名渠道而不是用 `channels` 会有一点帮助。`bundled-vocabulary` 默认关闭，因为它是
几百 KB 的 tokenizer。

## 构建

```sh
cargo install worker-build
worker-build --release -- --no-default-features --features d1,custom,codex,claudecode
```

`worker-build` 编译到 wasm、运行 `wasm-bindgen`、用 `wasm-opt -Oz` 优化，并写出
`wrangler.toml` 的 `main` 所指的那个 JS shim。它**不是可选的**：未经优化的构建会超出限制
好几倍。

| Feature | 增加什么 |
| --- | --- |
| `d1`（默认） | Cloudflare D1 store |
| `libsql` | 经 fetch 传输的 libSQL/Turso store |
| `s3` | 给发布 body 与词表用的 S3/R2 存储 |
| `bundled-vocabulary` | DeepSeek 词表 |
| `channels`（默认） | 全部 25 个；逐个点名可得到更小的二进制 |

## 配置

两级，没有更多。Worker 没有命令行、没有 `.env`、也没有文件。

1. **`GPROXY_CONFIG`**——一份 JSON 文档，形状与原生二进制的 `gproxy.toml` 完全相同。
   每个字段都可选，整个变量也可选。
2. **具名的 secret**，各自覆盖对应的字段。

这个拆分不是风格问题。`[vars]` 活在 `wrangler.toml` 里、活在版本库里、以明文形式；而
`wrangler secret put` 把值加密存放并且再也不显示它。主密钥属于第二个地方，所以把它挡在
第一个地方之外的唯一办法，就是让 secret 覆盖文档。

| Binding | 类型 | 是什么 |
| --- | --- | --- |
| `GPROXY_CONFIG` | var | 配置文档，JSON 形式 |
| `GPROXY_MASTER_KEY` | secret | 32 字节，64 个十六进制数字或 base64 |
| `GPROXY_LIBSQL_TOKEN` | secret | 用 libSQL store 时的 Turso bearer 令牌 |
| `GPROXY_S3_ACCESS_KEY_ID` | secret | 用 S3/R2 文件存储时 |
| `GPROXY_S3_SECRET_ACCESS_KEY` | secret | 用 S3/R2 文件存储时 |

没有 `GPROXY_CONFIG` 时，默认值是 `DB` 这个 D1 binding 和数据库承载的 cache——能服务流量的
最小组合。一份只点名了*部分*字段的文档，其余仍然拿到这两个默认值，因为共享类型自己的默认
描述的是原生二进制（一个 SQLite 文件、一个进程内 cache），而 Worker 两者都打不开。

`host`、`port`、`data_dir` 和 `console` 描述的是一个有 socket 和文件系统的进程。它们被接受
——这是一个共享类型——但从不被读取。

```toml
[vars]
GPROXY_CONFIG = """
{
  "store": { "kind": "d1", "binding": "DB" },
  "cache": { "kind": "store" },
  "public_base_url": "https://gproxy.example.workers.dev",
  "cors_origins": ["https://gproxy.example.workers.dev"],
  "trusted_proxies": [],
  "session_ttl_secs": 2592000,
  "oauth": { "access_ttl_secs": 3600, "cli_client_ids": [] }
}
"""
```

主密钥的编码按原生二进制用的同一条规则嗅探：正好 64 个十六进制数字是 hex，别的是 base64。
32 字节的 base64 是 43 或 44 个字符，所以两者不会混淆。不设时密钥以明文存放，并在每次冷
启动时打一次告警。

### 装配拒绝什么，以及为什么它拒绝而不是给个错答案

- **`cache: memory`。** 它是每个 isolate 一份的。放在那里的限流计数器永远只数到一，而写在
  那里的登录会话在跳转回来之前就没了。用 `store`（数据库当 cache）或 `redis`。
- **`store: sqlite` / `store: url`。** Worker 打不开文件，也开不了 TCP 连接。用 `d1` 或
  `libsql`。
- **`file_storage: fs`。** 没有文件系统。用 S3——R2 说 S3。

## 一次部署必须绑定什么

`crates/gproxy-host-edge/wrangler.toml.example` 是一份填好的副本。简版：

- 一个 **D1 数据库**，写成 `[[d1_databases]]`，binding 名就是 `GPROXY_CONFIG` 的
  `store.binding` 所指的那个（默认 `DB`）——或者改用一个 libSQL URL，并以
  `--features libsql` 构建；
- **`GPROXY_MASTER_KEY`**，除非明文密钥可以接受；
- **`public_base_url`**，如果有上游需要一个可抓取的链接来取发布出去的 body；
- **S3/R2 及其两个 secret**，用于发布 body 与下载的词表，并以 `--features s3` 构建；
- **Workers Assets**，如果 console 由这次部署提供。

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "00000000-0000-0000-0000-000000000000"

# [assets]
# directory = "console/dist"
# binding = "ASSETS"
# run_worker_first = ["/v1/*", "/admin/api/*", "/portal/api/*", "/healthz", "/publications/*"]
```

`run_worker_first` 把 API 路由留在 Worker 上，让其余的落到静态文件，这样 console 永远不会
唤醒一个 isolate，wasm 二进制也留在体积限制之内。

**迁移不是 Worker 的事。** 它跑在流量之下，而流量之下做 DDL 正是两个 isolate 把一次迁移
互相锁死的方式，所以 builder 从不碰表结构。部署之前跑
`wrangler d1 migrations apply`。

```sh
wrangler d1 create gproxy
wrangler d1 migrations apply gproxy --remote
wrangler secret put GPROXY_MASTER_KEY
wrangler deploy
```

## 同步就一行

isolate 不是进程。它在有流量时被创建、没流量时被销毁，而且**请求之间它里面什么都不跑**
——一个后台的订阅加轮询循环根本不会被 poll。

所以句柄以手动模式装配，而全部机制就是每个请求开头的 `tick()`：读一次配置 revision，
在它比这个 isolate 已发布的更新时重载两份快照。

失败的 `tick` 被记录并吞掉。它是一次*追赶*：此刻读不到 revision 的 isolate 服务的是它已经
有的那份配置，那是一个更早的 revision，而不是一个错的。拒绝反而会把一次短暂的数据库抖动
变成一次对并不需要那条未见写入的流量的中断。

装配在 isolate 的生命周期内被缓存，因为另一条路是每个请求做一次完整的配置加载。在第一次
装配完成前到达的两个请求会各装配一次，输的那份被丢掉，代价是一次白费的冷启动，别无其他
——因为 D1 与 libSQL 都是请求作用域的 HTTP，没有连接要保持。

## axum 怎么能跑在 Worker 里

三个事实，每一个都是靠编译而不是靠读文档确认的。

1. **axum 能为 `wasm32-unknown-unknown` 构建。** 它的 server 那几半是 Cargo feature 而不是
   crate 本身：`http1`/`http2` 是 hyper，`tokio` 是 serve 函数与连接信息，`ws` 是 hyper 的
   升级。关掉这些之后，router 和本网关用到的每个提取器都能编译。
2. **`worker` crate 的 `http` feature 去掉了适配层。** fetch 事件直接交出一个
   `http::Request` 并接受一个 `http_body::Body` 回去，而 router 正好在这两者上实现了 tower
   service。它们直接对接——这也是这个 crate **不**移植 v3 手写的 web-sys 适配器的原因：
   那些在 v3 是对的，而在这里它们会是 Cloudflare 自己维护的代码的第二份副本。
3. **`Send` 才是真正的障碍。** wasm 上引擎按设计是 `!Send`——JS 传输句柄属于创建它的
   isolate——而 axum 要求 `Send + Sync` 的 state 和 `Send` 的 handler future。

两处运行期检查的桥接解决了第三点，都成立于"Worker isolate 是单线程"：产品层把句柄放进一个
`SendWrapper`，axum 宿主包住每个 handler 体。**漏包的 handler 是 wasm 目标上一个指名道姓
的编译错误**——这就是强制手段，也是它放在 handler 而不是 router 的 `cfg` 上的原因。编不过
edge 的路由，不可能在 edge 上悄悄不存在。

## 测了什么，没测什么

配置那一半是纯数据，它的测试跑在宿主目标上：

```sh
cargo test -p gproxy-host-edge
```

它们覆盖文档形状、edge 默认值、secret 覆盖、主密钥编码嗅探，以及上面四种拒绝的每一种。

其余都在 `cfg(target_arch = "wasm32")` 后面，靠编译来验证：

```sh
cargo clippy -p gproxy-host-edge --target wasm32-unknown-unknown --lib -- -D warnings
```

**未被演练过的**：fetch 入口、isolate 装配、D1 binding 查找、libSQL 传输和 `tick()` 都从未
真正跑过，因为跑它们需要一个 Workers 运行时。它们下面的那张路由表是原生宿主的，由它的
测试覆盖。
