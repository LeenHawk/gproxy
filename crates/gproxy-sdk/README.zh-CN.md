# gproxy-sdk

[English](README.md) | 简体中文

宿主嵌入用的 GPROXY 句柄。`gproxy-core` 只负责按调用方给定的 Provider 执行请求：
它不写配置、不解析模型名、也不知道别的实例改了什么。这一层补上其余部分——用默认
实现装配出 `Core`，承担推进 `settings.config_revision` 的配置写入，把一次登录变成
一行凭证，把模型名解析成执行计划，并通过共享 cache 与持久 revision 轮询让同一部署
的每个实例停在同一个 revision 上。

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

`build()` 不会打开任何未被交给它的东西：同步实体 schema（除非关掉）、创建全局
settings 行、装配引擎、载入首个快照，并在 `SyncMode::Background` 下启动订阅与轮询
循环。密钥编解码器永不隐式选择：`master_key` 用 AES-256-GCM 密封，
`plaintext_secrets` 是显式的明文选项，两者都没有则直接拒绝构建。

## Features

| Feature | 作用 |
|---|---|
| `custom` / `codex` / `claudecode` / `claudeweb` | 编入对应渠道并默认注册 |
| `postgres` / `mysql` | 额外的 SeaORM 驱动，仅原生；原生始终自带 SQLite |
| `libsql` | 经 Hrana HTTP pipeline 的 libSQL／Turso，全平台 |
| `d1` | Cloudflare D1 绑定的标记，wasm32 本就自带 |
| `memory`（默认） | 进程内 `MemoryCache`，原生默认 cache |
| `redis` | Redis／Valkey，多实例部署用 |
| `fs`（默认） | 本地文件系统对象存储，仅原生 |
| `s3` | S3／R2 对象存储 |
| `bundled-vocabulary`（默认） | 内置 DeepSeek 词表用于 token 估算 |
| `ts` | 为 DTO 生成 `ts-rs` 声明 |

默认集可在 `wasm32-unknown-unknown` 上编译，`libsql` 亦然。wasm 上 cache 默认是
`gproxy_store::StoreCache`，同步一律手动：isolate 活不过一次请求，在请求开头调用
`tick()` 就是全部机制。

原生构建使用 `reqwest` 传输。想要别的后端（`wreq` 做 TLS 指纹、`reqwest-native`
对齐 Codex CLI 的栈）的宿主自行在 `gproxy-client` 上打开该 feature，或把自己的
`ClientPool` 交给 builder。

## 同步

两套机制，缺一不可。

| | 传递什么 | 会怎么失败 |
|---|---|---|
| 共享 cache 上的 `Invalidation` | 毫秒级的“再看一眼” | 丢消息：发布失败、订阅滞后、topic 关闭 |
| `settings.config_revision` 轮询 | 持久事实，默认 30s 一次 | 慢 |

通知从不携带状态：它只说存在哪个 revision，实例仅在该值比自己正在服务的更大时才
重载。无法解析的载荷、或指向本快照从未见过的凭证的通知，一律重载而不是猜测。
重载是串行且单调的——旧 revision 永远不会覆盖新的，重载失败则继续服务旧快照。

## 不在这里的东西

- **身份**：用户、API key、组织、团队、权限、订阅、限流与 OAuth issuer 属于上层
  应用，它通过 `gproxy_store::load_all_data` 读同一个持久 revision。
- **下游鉴权**：这里不判断调用方是谁；句柄拿到的是 scope 以及允许的 Provider 与
  凭证集合。
- **server**：没有监听、路由、中间件或 CLI。原生与边缘宿主建在本 crate 之上。
- **执行**：尝试、协议转换、改写、观测、预算与结算都属于 `gproxy-core`，本 crate
  只决定交给它什么。
