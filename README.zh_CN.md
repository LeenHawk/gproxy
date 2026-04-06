# 部署指南

### 编译

单实例构建：

```bash
cargo build -p gproxy --release
```

多实例构建，启用 Redis backend：

```bash
cargo build -p gproxy --release --features redis
```

构建产物位于 `target/release/gproxy`。

### 环境变量

`apps/gproxy/src/main.rs` 中定义了完整的启动参数和对应环境变量：

| 环境变量 | 默认值 | 必填 | 说明 |
| --- | --- | --- | --- |
| `GPROXY_HOST` | `127.0.0.1` | 否 | 监听地址。 |
| `GPROXY_PORT` | `8787` | 否 | 监听端口。 |
| `GPROXY_ADMIN_USER` | `admin` | 否 | 创建或对齐 bootstrap 管理员账号时使用的用户名。 |
| `GPROXY_ADMIN_PASSWORD` | 无 | 否 | bootstrap 管理员密码。首次启动需要创建管理员且未提供时，会自动生成并只在日志中输出一次。 |
| `GPROXY_ADMIN_API_KEY` | 无 | 否 | bootstrap 管理员 API Key。首次启动需要创建管理员且未提供时，会自动生成并只在日志中输出一次。 |
| `GPROXY_DSN` | 若未设置，则自动生成 `sqlite://<data_dir>/gproxy.db?mode=rwc`。 | 否 | 数据库连接串。 |
| `GPROXY_PROXY` | 无 | 否 | 上游 HTTP 代理。 |
| `GPROXY_SPOOF` | `chrome_136` | 否 | TLS 指纹模拟名称。 |
| `DATABASE_SECRET_KEY` | 无 | 否 | 数据库存储加密密钥；设置后，凭证、密码和 API Key 会以 XChaCha20Poly1305 方式静态加密。 |
| `GPROXY_REDIS_URL` | 无 | 否 | Redis 连接串；只有编译了 `redis` feature 时才会真正启用 Redis backend。 |
| `GPROXY_CONFIG` | `gproxy.toml` | 否 | 首次初始化时用于 seed 的 TOML 配置文件路径。 |
| `GPROXY_DATA_DIR` | `./data` | 否 | 数据目录；默认 SQLite 文件和运行期数据都基于这个目录。 |

补充说明：

- CLI 参数和环境变量都由 `clap` 解析，命令行显式传参优先于默认值。
- 如果数据库里已经有 `global_settings`，且启动时没有显式传入 `GPROXY_DSN` / `GPROXY_DATA_DIR`，进程会按持久化配置重新连接数据库。

### TOML 配置文件格式

`GPROXY_CONFIG` 指向的 TOML 只在"数据库没有现成数据"时参与初始化。对应结构定义在 `crates/gproxy-api/src/admin/config_toml.rs`。

```toml
[global]
host = "0.0.0.0"
port = 8787
proxy = "http://127.0.0.1:7890"
spoof_emulation = "chrome_136"
update_source = "github"
enable_usage = true
enable_upstream_log = false
enable_upstream_log_body = false
enable_downstream_log = false
enable_downstream_log_body = false
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"

[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [
  { api_key = "sk-provider-1" }
]

[[models]]
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
display_name = "GPT-4.1 mini"
enabled = true
price_each_call = 0.0

[[model_aliases]]
alias = "chat-default"
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true

[[users]]
name = "alice"
password = "plain-text-or-argon2-phc"
enabled = true

[[users.keys]]
api_key = "sk-user-1"
label = "default"
enabled = true

[[permissions]]
user_name = "alice"
provider_name = "openai-main"
model_pattern = "gpt-*"

[[file_permissions]]
user_name = "alice"
provider_name = "openai-main"

[[rate_limits]]
user_name = "alice"
model_pattern = "gpt-*"
rpm = 60
rpd = 10000
total_tokens = 200000

[[quotas]]
user_name = "alice"
quota = 100.0
cost_used = 0.0
```

字段说明：

- `[global]` 对应全局监听、日志、更新源、DSN 和数据目录配置。
- `[[providers]]` 定义 Provider；`settings` 与 `credentials` 都是 JSON 值，经 `serde_json::Value` 读取。
- `[[models]]` / `[[model_aliases]]` 定义可转发模型和别名。
- 管理员身份通过 `[[users]]` 中 `is_admin = true` 且至少有一个启用中的 `[[users.keys]]` 来表示。如果 seed 配置里没有这样的管理员，启动时可以用 `GPROXY_ADMIN_USER`、`GPROXY_ADMIN_PASSWORD`、`GPROXY_ADMIN_API_KEY` 自动补一个 bootstrap 管理员。
- `[[users]]` 下的 `password` 既可以是明文，也可以直接是 Argon2 PHC hash。
- `[[users.keys]]` 是嵌套数组表，表示该用户的 API Key 列表。
- `[[permissions]]`、`[[file_permissions]]`、`[[rate_limits]]`、`[[quotas]]` 分别对应模型权限、文件权限、限流和成本配额。

### 数据库支持

`gproxy-storage` 通过 SeaORM / SQLx 编译进了三类数据库支持：

| 数据库 | DSN 前缀 | 说明 |
| --- | --- | --- |
| SQLite | `sqlite:` | 默认模式；如果未显式设置 `GPROXY_DSN`，启动时会自动生成 SQLite 文件 DSN。 |
| PostgreSQL | `postgres:` | 由 `sqlx-postgres` / SeaORM Postgres feature 提供。 |
| MySQL | `mysql:` | 由 `sqlx-mysql` / SeaORM MySQL feature 提供。 |

常见 DSN 示例：

```text
sqlite://./data/gproxy.db?mode=rwc
postgres://gproxy:secret@127.0.0.1:5432/gproxy
mysql://gproxy:secret@127.0.0.1:3306/gproxy
```

连接建立后，`SeaOrmStorage::connect()` 会：

1. 可选加载 `DATABASE_SECRET_KEY` 对应的数据库加密器。
2. 按数据库类型应用连接优化参数。
3. 连接数据库并执行 `sync()` 以同步 schema。

### 多实例部署

#### 基本方式

当前仓库支持"共享数据库 + 可选共享 Redis backend"的多实例部署：

1. 使用同一套数据库 DSN。
2. 所有实例都编译 `--features redis`。
3. 所有实例都设置同一个 `GPROXY_REDIS_URL`。

#### Redis backend 当前覆盖范围

按 `apps/gproxy/src/main.rs` 的实际注入逻辑，当前二进制会在运行时启用：

- `RedisQuota`
- `RedisRateLimit`

`gproxy-core` 里虽然还实现了 `RedisAffinity`，但当前 `apps/gproxy` 没有把它注入 `AppStateBuilder`，所以本仓库现状下它还不是已启用的共享 backend。

#### 什么是共享的，什么是本地的

共享状态：

- 数据库中的持久化数据：`global_settings`、providers、credentials、models、aliases、users、keys、permissions、file permissions、usages、request logs、credential statuses、用户文件记录等。
- Redis 中的运行态数据：用户 quota reservation / settle 状态，以及 rate-limit 计数窗口。

本地状态：

- 每个进程自己的 `AppState` 内存快照：Identity、Policy、Routing、File、Config、QuotaService。
- SDK engine、上游连接池、worker 缓冲区、shutdown 信号、日志上下文。
- 未启用 Redis 时的 rate-limit counter。

根据源码可推断出的同步行为：

- providers、models、aliases、users、keys、permissions、file permissions 这些缓存，主要在启动或 `POST /admin/reload` 时刷新。
- quota 额外有一个每 30 秒运行一次的 `QuotaReconciler`，会把数据库配额修正回本地内存。
- 因此，多实例场景下如果你在某个实例上改了管理配置，其他实例通常还需要执行 `/admin/reload` 或重启，才能立刻看到新缓存；quota 是少数有周期性自愈同步的例外。

### Graceful Shutdown

优雅停机行为由 `apps/gproxy/src/main.rs` 和 `apps/gproxy/src/workers/mod.rs` 共同实现：

1. 进程监听 `Ctrl+C`；在 Unix 上还监听 `SIGTERM`。
2. 触发停机后，Axum server 进入 `with_graceful_shutdown` 流程，不再继续正常服务。
3. 主线程随后调用 `worker_set.shutdown()`，向所有 worker 广播关闭信号。
4. `WorkerSet` 最多等待 5 秒让 worker 排空。
5. `UsageSink` 会关闭接收端、吸干剩余 usage 消息并执行最后一次批量写入。
6. `HealthBroadcaster` 会把防抖窗口里尚未落库的健康状态补写到数据库。
7. `QuotaReconciler` 和 `RateLimitGC` 收到信号后直接退出下一轮循环。
8. 如果 5 秒内还有 worker 没结束，进程会记录 warning，但不会无限阻塞。
