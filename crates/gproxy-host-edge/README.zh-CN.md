# gproxy-host-edge

[English](README.md) | 简体中文

GPROXY v4 的 Cloudflare Workers 宿主：架在
[`gproxy-host-axum`](../gproxy-host-axum) 路由之上的一个 `fetch` 处理器。

**这个 crate 里没有路由表。** 它挂的就是原生二进制在跑的那张 `axum::Router`，
同样的路由、同样的 handler。axum 宿主新增一条路由，这个宿主就多服务一条，
这里不用改。

```rust,ignore
#[event(fetch)]
async fn fetch(request: HttpRequest, env: Env, _ctx: Context)
    -> worker::Result<http::Response<axum::body::Body>>
{
    let instance = instance(&env).await?;   // 每 isolate 装配一次
    instance.tick().await;                  // 追上其它实例写的配置
    Ok(instance.router().call(request).await?)
}
```

这就是宿主的全部。其余都是装配与配置。

## axum 的 Router 真能跑在 Worker 里吗

这是开工前必须先答的问题，答案是能——但不是白来的。为什么不选另一条路，值得写下来。

**被否掉的做法。** v3 有 `app.admin_dispatch(&parts, body)`，一张按字符串键的
分派表，两个宿主都调它。v4 删掉了它，理由写在
[`gproxy-host-axum` 的 `admin` 模块](../gproxy-host-axum/src/admin.rs)里：没有任何
分支匹配上的路径会穿透下去，变成一个看起来像"行不存在"的 404。那个显而易见的替代
方案——在这个 crate 里把四十多条管理面路由再手写一遍——是同一种失败，只是引线更长：
两张表漂移，直到某个控制台按钮只在生产环境 404 才有人发现。

**为什么行得通。** 三件事，每一件都是编译出来的，不是读文档读出来的：

1. **axum 能编译到 `wasm32-unknown-unknown`。** 它的 server 那几半是 feature，
   不是 crate 本体：`http1`/`http2` 是 hyper，`tokio` 是 `axum::serve` 与
   `ConnectInfo`，`ws` 是 hyper 升级。`default-features = false` 加上 `json`、
   `query`、`matched-path`，`Router` 和这个网关用到的每个 extractor 都能编。
   `Router` 本身与目标无关。
2. **`worker` 的 `http` feature 把适配层整层去掉了。** 开了之后
   `#[event(fetch)]` 直接给 `http::Request<worker::Body>`，回任意
   `http_body::Body<Data = Bytes>`，而 `Router<()>` 实现了
   `tower::Service<http::Request<B>>`。两边直接对接。这也是本 crate **没有**移植
   v3 那套手写 `web_sys::Request`／`Response`／`ReadableStream`／header 适配器的
   原因：它们对 v3 是对的（v3 自己说 web-sys），在这里会变成 Cloudflare 自己在维护的
   那份代码的第二份拷贝。
3. **真正的障碍是 `Send`，`send_wrapper` 把它桥掉了。** wasm 上引擎按设计就是
   `!Send`——JS 传输句柄属于创建它的 isolate，`gproxy-client` 的 `ClientBounds`
   写的正是这件事（原生 `Send + Sync`，wasm 上为空）——而 axum 要
   `Router<S>: S: Clone + Send + Sync + 'static`（`axum-0.8.9/src/routing/mod.rs:89`）、
   `Handler::Future: Send`（`handler/mod.rs:148`）、
   `Body::from_stream<S>: S: TryStream + Send`（`axum-core-0.5.6/src/body.rs:61`）。

   不桥接的话，`gproxy_app::App<C>` 在 wasm 上既不 `Send` 也不 `Sync`，链路是
   `App<C>` → `Gproxy<C>` → `gproxy_sdk::handle::Inner<C>` →
   `gproxy_core::Core<C>` → `gproxy_client::pool::wasm::ClientPool`——后者持有
   `HashMap<_, Arc<Client>>`（元素是 `dyn OutboundClient`）和一个
   `Arc<dyn Fn(&ConnectionConfig, bool) -> Result<Client, Error>>`。

   两处改动解决它，都是运行时检查而不是断言，也都因为 Worker isolate 是单线程而成立：
   `gproxy_app::App` 用 `SendWrapper` 持有句柄，`gproxy_host_axum::send` 包住每个
   handler 的函数体。漏包的 handler 是 **wasm 目标上一条点名到它的编译错误**——这就是
   强制机制，也是为什么包在 handler 上而不是给路由加 `cfg`：编译不过的路由，不可能在
   边缘悄无声息地"不存在"。

## 这个宿主不提供什么

| 面 | 原因 |
|---|---|
| websocket / 实时 | Worker 的升级方式是造一个 `WebSocketPair`、把客户端那一半塞进 `Response` 的 `webSocket` 字段——这套机制根本没有 `http::Response` 的形状。握手回 `501`。 |
| 内嵌 console | 打包产物归 Workers Assets，挡在这个 Worker 前面；见 `wrangler.toml.example`。 |

其余全是 axum 路由的，原样：数据面及各渠道的 service 路由、OAuth issuer、
`/admin/api`、`/portal/api`、`/publications/{id}`、`/healthz`。

要把实时做成真的，缺两块，都不是路由问题：一个 `WebSocketPair` 版的
`gproxy_protocol::connection::WebSocket` 实现，以及一条让 handler 把备好的 JS
`Response` 交回 fetch 入口的路。第二条已经有现成的缝——`http::Extensions` 收
`Send + Sync + Clone` 的值，而 `SendWrapper<web_sys::Response>` 三样都满足。
这里故意没做：一条从没对着真客户端跑过的升级路径，不如一次诚实的拒绝值钱。

## 同步只有一行

isolate 不是进程。有流量时被创建，没流量时被销毁，而且**两次请求之间它里面什么都不跑**
——后台订阅加轮询的循环根本不会被 poll。所以 sdk 用 `SyncMode::Manual` 装配，整个机制
就是每个请求开头的 `tick()`：读一次 `settings.config_revision`，比本 isolate 已发布的
更新就重载两份快照。

`tick` 失败会记日志然后咽下去。它是一次**追赶**：此刻读不到 revision 的 isolate，
服务的是它已经加载的那份配置——是旧 revision，不是错 revision。改成拒绝请求，等于
把数据库的一次抖动变成一次故障，而那些流量本来并不需要那条还没看见的写入。

装配结果缓存在 `OnceLock` 里、活到 isolate 结束，因为另一种选择是每个请求都做一次
完整配置加载。第一次装配还没完成时到达的两个请求会各装配一次；输的那份被丢弃，
代价是一次白费的冷启动，仅此而已——D1 和 libSQL 都是请求级 HTTP，没有连接要占着。

## 配置

两层，不再多。Worker 没有命令行、没有 `.env`、没有文件。

1. **`GPROXY_CONFIG`**——一份 JSON，形状就是 `AppConfig`，和原生二进制的
   `gproxy.toml` 是同一个类型。每个字段都可省，整个变量也可省。
2. **具名 secret**，各自覆盖对应字段。

这个切分不是风格问题：`[vars]` 躺在 `wrangler.toml` 里、进仓库、明文；
`wrangler secret put` 把值加密存起来且再也不显示。主密钥属于后者，所以要让它进不了
前者，唯一的办法就是让 secret 覆盖文档。

| Binding | 类型 | 含义 |
|---|---|---|
| `GPROXY_CONFIG` | var | `AppConfig` 文档，JSON |
| `GPROXY_MASTER_KEY` | secret | 32 字节，64 位十六进制或 base64 |
| `GPROXY_LIBSQL_TOKEN` | secret | libSQL 存储时的 Turso bearer token |
| `GPROXY_S3_ACCESS_KEY_ID` | secret | 配了 S3/R2 存储时 |
| `GPROXY_S3_SECRET_ACCESS_KEY` | secret | 配了 S3/R2 存储时 |

没有 `GPROXY_CONFIG` 时，默认是 `DB` 这个 D1 binding 加数据库当 cache——能跑起流量的
最小组合。只写了**一部分**字段的文档，没写的那两项照样拿边缘默认值，因为
`AppConfig::default()` 描述的是原生二进制（一个 SQLite 文件、一个进程内 cache），
Worker 哪个都打不开。

主密钥的编码是嗅出来的，规则和原生二进制一致：正好 64 位十六进制就是 hex，
其余按 base64。32 字节的 base64 是 43 或 44 个字符，两者不可能混淆。不设则密文按
明文存，每次冷启动警告一次。

### 装配会拒绝什么，以及为什么是拒绝而不是答错

- **`cache: memory`。** 它是每 isolate 一份。限流计数器在那里永远只数到 1，
  写进去的登录会话在重定向回来之前就没了。用 `store`（数据库当 cache）或 `redis`。
- **`store: sqlite` / `store: url`。** Worker 打不开文件，也开不了 TCP。用 `d1` 或 `libsql`。
- **`file_storage: fs`。** 没有文件系统。用 S3——R2 说这套协议。

### 一次部署必须绑定什么

`wrangler.toml.example` 是填好的一份；简版：

- 一个 **D1 数据库**，写成 `[[d1_databases]]`，binding 名与 `GPROXY_CONFIG` 里
  `store.binding` 一致（默认 `DB`）——或者改用 libSQL URL，crate 带 `--features libsql` 构建；
- **`GPROXY_MASTER_KEY`**，除非能接受明文存密文；
- **`public_base_url`**，如果有上游需要一个可抓取的链接来取已发布内容；
- **S3/R2 及其两个 secret**，用于已发布内容和下载的词表，crate 带 `--features s3` 构建；
- **Workers Assets**，如果 console 也从这个部署提供。

**迁移不归 Worker 管。** 它在流量下运行，而在流量下做 DDL 正是两个 isolate 把迁移
卡死的方式，所以 builder 用 `build_unsynced` 装配，完全不碰 schema。部署前跑
`wrangler d1 migrations apply`。

## 构建

```sh
cargo install worker-build
worker-build --release -- --no-default-features --features d1,custom,codex,claudecode
```

`worker-build` 会编到 wasm、跑 `wasm-bindgen`、用 `wasm-opt -Oz` 优化，并写出
`wrangler.toml` 里 `main` 指向的那个 JS shim。

**体积是真实约束，而且这个 crate 离上限不远。** 在本仓库用
`cargo build --release --target wasm32-unknown-unknown` 实测，`wasm-bindgen` 与
`wasm-opt` 之前：

| 构建 | 原始 | gzip |
|---|---|---|
| `--features channels`（全部 25 个） | 25.9 MB | 7.7 MB |
| `--features d1,custom,codex,claudecode` | 24.8 MB | 7.4 MB |

Cloudflare 限的是压缩后的包体——免费 3 MB，付费 10 MB。`wasm-opt -Oz` 和
`wasm-bindgen` 的 gc 能砍掉相当一部分，上面的数字里没有它们；**没有做过任何一次实测
部署，因为本仓库没有 Cloudflare 账号也没有 wrangler 工具链。** 部署方必须自己量自己的
包。逐个点名渠道而不是用 `channels` 能省一点；`bundled-vocabulary` 默认关着，
因为那是几百 KB 的分词词表。

## Feature

| Feature | 加了什么 |
|---|---|
| `d1`（默认） | Cloudflare D1 存储 |
| `libsql` | 经 fetch 传输的 libSQL／Turso 存储 |
| `s3` | S3/R2 对象存储，用于已发布内容与词表 |
| `bundled-vocabulary` | DeepSeek 词表，给目录里没有词表文件的模型用 |
| `channels`（默认） | 本仓库实现的全部渠道；想要小二进制就逐个点名 |

## 测了什么，没测什么

`config.rs` 是纯数据，测试跑在宿主目标上（`cargo test -p gproxy-host-edge`）：
文档形状、边缘默认值、secret 覆盖、主密钥编码嗅探，以及上面那四种错误配置各一条。

这个 crate 其余部分都在 `cfg(target_arch = "wasm32")` 之下，靠编译验证——
`cargo clippy -p gproxy-host-edge --target wasm32-unknown-unknown --lib -- -D warnings`
以及一次真正产出产物的 `cargo build`。**没有被执行过的**：fetch 入口、isolate 装配、
D1 binding 查找、libSQL 传输和 `tick()`，跑它们需要一个 Workers 运行时。它们底下那张
路由表是原生宿主的，由原生宿主的测试覆盖。
