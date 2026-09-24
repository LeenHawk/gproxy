# `gproxy`

GPROXY v4 的命令行：运维真正启动的那个进程。

这个 crate 之下的每一层都是「配置由参数传入」的库。读环境变量、打开文件、绑定套接字、
往终端打印——只有这一层做这些事。

- [`gproxy-sdk`](../gproxy-sdk) 装配引擎。
- [`gproxy-app`](../gproxy-app) 负责身份、准入和 typed 操作。
- [`gproxy-host-axum`](../gproxy-host-axum) 把它们变成 HTTP 表面。

```
cargo run -p gproxy -- serve
```

首次启动会建库、建表、建管理员和一把 API key，并把两个密钥各打印一次。

---

## 命令

| 命令 | 作用 |
|---|---|
| `gproxy serve` | 按需轮换主密钥、实例是新的就 bootstrap、绑定、服务。**默认命令**——不带子命令的 `gproxy` 就是 `gproxy serve`。 |
| `gproxy migrate` | 创建 schema 或应用待执行迁移，然后退出。 |
| `gproxy bootstrap admin` | 建第一个管理员。幂等。 |
| `gproxy export --out <PATH>` | 把本实例的配置写成一份 JSON 文档。 |
| `gproxy import --in <PATH>` | 把这样一份文档回放进本实例。 |
| `gproxy service install` | 写出本机自己的服务单元，启用并启动它。 |

所有配置项都是全局的：`gproxy --port 9000 serve` 和 `gproxy serve --port 9000`
是同一次调用。

### `serve`

```
gproxy serve --host 0.0.0.0 --port 8787 --data-dir /var/lib/gproxy
```

绑定、服务，收到 `SIGINT` 或 `SIGTERM` 时停止——先把在途请求排空。**没有关闭超时**：
一个流式补全跑几分钟是合理的，需要截止时间的 supervisor 有 `TimeoutStopSec`。

绑定发生在 schema 工作**之后**。这样一个还在迁移的进程会直接拒绝连接，而不是把连接
收进一个没人应答的 backlog。

### `migrate`

```
gproxy migrate --dsn postgres://gproxy@db/gproxy
```

`serve` 也会执行同一套 SeaORM 迁移。空库创建当前 schema 并写入
`seaql_migrations`；已有库按账本执行待应用迁移，不再自动猜测表结构变化。
有表却没有账本，或账本比当前构建新时，会在 DDL 前拒绝。
`gproxy migrate --status` 只读取迁移状态，不创建表。
单独的命令是给「把迁移当作独立步骤、单写者、在任何实例
启动之前跑」的部署用的——多实例共用一个数据库时必须这样做。

### `bootstrap admin`

```
gproxy bootstrap admin --user admin --password "…" --api-key "sk-…"
```

`--user`、`--password`、`--api-key` 是全局的 `--admin-user`、`--admin-password`、
`--admin-api-key` 的别名，所以同样的值对 `serve` 也有效。

### 从 v3 自动迁移

保持原数据目录和 `GPROXY_MASTER_KEY`，按原命令启动即可。v4 自动识别 v3 SQLite，
先在旁边完成转换，成功后替换原路径并保留 `gproxy.db.v3-*.bak`。原密码与 API key
继续有效，失败时不切换数据库。`gproxy migrate` 也走同一路径。

`import --from-v3` 仅用于另外导入备份或 JSON，不是正常升级的必需步骤。
详见[迁移指南](../../docs/src/content/docs/zh-cn/deployment/v3-to-v4.md)。

### `export` / `import`

```
gproxy export --out config.json --include-secrets
gproxy import --in config.json --mode merge --source-master-key "…"
```

`-` 表示标准输出或标准输入。

会迁移的是「一个部署**本身**的配置」：连接档案、Provider、凭证、模型目录、路由、
操作覆盖、改写规则、限额、定价和 settings 行。**身份不迁移**——用户、key、组织、团队、
权限、订阅属于应用层——所以导入后的实例仍然需要自己的 bootstrap。用量和 capture
也不迁移：复制它们等于伪造目标实例从未有过的历史。

`--mode merge` 写文档里提到的行，其余不动。`--mode replace` 额外删除「属于已导出
种类、但文档没有提到」的行。

带 `--include-secrets` 时，密封后的凭证 blob 以 base64 随行，全程不解封、绝不明文——
文档因此与数据库文件同等敏感。回放时：

| 导入方拥有 | 结果 |
|---|---|
| `--source-master-key` | 每个密钥被解开并用本实例的密钥重新密封 |
| 与来源相同的密钥 | blob 原样存入，本来就能解开 |
| 两者都没有 | 该凭证被跳过、计数并告警 |

### `service`

```
gproxy service install [--autostart] [--port …] [--data-dir …]
gproxy service uninstall
gproxy service status
```

**没有 `--daemon`，没有 pidfile，也没有 `gproxy stop`。** 后台化、崩溃后重启、开机
自启，这三件事每个操作系统都已经解决过了；内置守护模式意味着要自己维护一个会变陈旧
的 pidfile、一份没人轮转的日志、一个没有退避的重启循环，以及一个与「还占着端口的
那个东西」赛跑的停止命令。所以 `gproxy serve` 始终是收到 `SIGTERM` 就退出的前台
进程，而 `gproxy service install` 写出那份「专门干这活的 supervisor」要读的单元。

单元复现的是**这一次**调用：`gproxy service install --port 9000 --data-dir
/srv/gproxy` 和 `gproxy serve --port 9000 --data-dir /srv/gproxy` 是同一个实例。
相对路径先被转成绝对路径——单元运行时的工作目录是 init 系统挑的，不是你的。

| 平台 | 写出什么 | 由谁启动 |
|---|---|---|
| Linux + systemd | `~/.config/systemd/user/gproxy.service` | `systemctl --user enable --now`；带 `--autostart` 时再加 `loginctl enable-linger` |
| macOS | `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist` | `launchctl bootstrap gui/<uid>` |
| Windows | 一个名为 `gproxy` 的登录触发计划任务 | `schtasks /create /xml` |
| Termux | `~/.termux/boot/gproxy.sh` | **Termux:Boot** 附加组件，必须另行安装 |

这四样都没有的机器——容器、chroot、基于 runit 或 s6 的发行版、不是 Termux 的
Android 应用——会被明确告知原因，而不是拿到一份没人会读的单元。

#### 单元里绝不会有什么

**密钥。** `~/.config/systemd/user/gproxy.service` 是一个普通文件，跟它能解开的
数据库只隔一个目录，`$HOME` 的每次备份都会带上它，而 `systemctl --user show`
会把它念给任何人听。所以单元携带的是环境文件的**路径**，绝不是其中的值：systemd
用 `EnvironmentFile=-`，另外三家用 `GPROXY_ENV_FILE`。

`install` 会说明它遇到的是哪一种：

| 主密钥来自 | `install` 的做法 |
|---|---|
| gproxy 会读的那个环境文件 | 只写文件名，密钥留在文件里 |
| 导出的环境变量，或 `--master-key` | **拒绝复制**，并指出该放进哪个文件 |
| 哪里都没有 | 告知该服务将以明文存储密钥 |

进入单元命令行的只有 `--host`、`--port`、`--data-dir` 和 `--config`。其余的——
带密码的 DSN、Redis URL、管理员密码——属于环境文件或配置文件，因为那是运维能给它
设权限位的文件。

#### `--autostart`

装上之后，服务总是「你登录时启动」。`--autostart` 要的是另一件事：没人登录也在跑。

- **systemd** —— `loginctl enable-linger`。`uninstall` **只在** linger 是被
  `install --autostart` 打开时才把它关回去，这一点由单元自己的一行注释记录。
- **macOS** —— `LaunchAgent` 做不到，而且什么也没做。开机就跑意味着 root 的
  `LaunchDaemon`，而一个用 root 写自己数据库的网关，比一个等你登录的网关更糟。
- **Windows** —— 不提权就做不到。开机即启的任务要么以 SYSTEM 身份运行，要么把你的
  密码存进任务库。
- **Termux** —— 本来就是。Termux:Boot 就是个开机钩子，仅此而已。

`status` 报告的是 init 系统说了什么——loaded、active、enabled、上次退出、重启
次数——而不是这个命令希望是什么：

```
$ gproxy service status
gproxy, as the systemd user manager sees it.
  unit       /home/leen/.config/systemd/user/gproxy.service
  loaded     loaded
  active     inactive
  sub-state  dead
  enabled    enabled
  restarts   1
  result     success
  last exit  exited with status 0
  linger     yes
```

---

## 配置

五个来源，由强到弱：

1. **命令行** —— `--port 9000`
2. **真实环境变量** —— `GPROXY_PORT=9000`
3. **`.env`** —— 加载时不覆盖环境里已有的任何键
4. **`--config` 指定的 TOML 文件**
5. **内置默认值**

命令行胜过环境变量，环境变量胜过 `.env`，`.env` 胜过文件。把文件排在环境变量**之下**
是关键选择：文件是一个部署签入仓库的意图，环境变量是某台主机、某个容器偏离它的方式。
如果文件赢了，compose 文件里的 `GPROXY_PORT` 就会悄无声息地不起作用。

分层只在启动时发生一次。入口点之下没有任何模块读 `std::env`。

### 环境变量

每个值都是一个带 `env = …` 的 `clap` 字段，所以 `--help` 会把环境变量印在它所遮蔽的
flag 旁边，这张表不可能和程序本身走偏。v3 里存在过的名字含义不变：升级不等于重新部署。

| 变量 | Flag | 默认 | 含义 |
|---|---|---|---|
| `GPROXY_CONFIG` | `--config`、`-c` | — | 可写任意 `AppConfig` 字段的 TOML 文件 |
| `GPROXY_HOST` | `--host` | `127.0.0.1` | 监听地址（IP，不是主机名） |
| `GPROXY_PORT` | `--port`、`-p` | `8787` | 监听端口 |
| `GPROXY_DATA_DIR` | `--data-dir` | `data` | 相对路径的解析根 |
| `GPROXY_PERSISTENCE` | `--persistence` | `sqlite` | `sqlite`、`postgres` 或 `mysql`（`db` 是 v2 对 `sqlite` 的叫法） |
| `GPROXY_DSN` | `--dsn` | — | 连接串；未给 `--persistence` 时由它的 scheme 决定后端 |
| `GPROXY_REDIS_URL` | `--redis-url` | — | 共享缓存；**多实例必需** |
| `GPROXY_MASTER_KEY` | `--master-key` | — | 32 字节，64 位十六进制或 base64。不设则明文存储 |
| `GPROXY_MASTER_KEY_NEXT` | `--master-key-next` | — | 要重新密封到的新密钥 |
| `GPROXY_MASTER_KEY_ROTATE` | `--master-key-rotate` | `false` | 启动时执行轮换 |
| `GPROXY_PUBLIC_BASE_URL` | `--public-base-url` | — | 对外 origin，用于发布链接和 OAuth issuer 标识 |
| `GPROXY_CORS_ORIGINS` | `--cors-origin` | — | 逗号分隔的浏览器 origin；空表示仅同源 |
| `GPROXY_TRUSTED_PROXIES` | `--trusted-proxy` | — | 逗号分隔、其 `x-forwarded-*` 可信的对端；空表示谁都不信 |
| `GPROXY_FILE_STORAGE_DIR` | `--file-storage-dir` | — | 发布体与词表的本地目录 |
| `GPROXY_CONSOLE` | `--console` | `true` | 是否提供控制台 |
| `GPROXY_CONSOLE_PATH` | `--console-path` | — | 从该目录提供控制台，而不是内嵌产物 |
| `GPROXY_INSTANCE_ID` | `--instance-id` | 随机 | 本进程的稳定名字 |
| `GPROXY_LOG_FORMAT` | `--log-format` | `text` | `text` 或 `json` |
| `GPROXY_LOG_FILTER` | `--log-filter` | `RUST_LOG`，否则 `info` | tracing 过滤器 |
| `GPROXY_ADMIN_USER` | `--admin-user` / `--user` | `admin` | 第一个管理员的名字 |
| `GPROXY_ADMIN_PASSWORD` | `--admin-password` / `--password` | 自动生成 | 其密码 |
| `GPROXY_BOOTSTRAP_ADMIN_API_KEY` | `--admin-api-key` / `--api-key` | 自动生成 | 为其铸造的确切 API key |
| `GPROXY_IMPORT_SOURCE_MASTER_KEY` | `--source-master-key` | — | 仅 `import`：来源实例的主密钥 |
| `GPROXY_AUTOSTART` | `--autostart` | `false` | 仅 `service install`：没人登录时也运行 |
| `GPROXY_ENV_FILE` | — | `.env` | 加载哪个 `.env` |

`GPROXY_ENV_FILE` 故意没有 flag：flag 要由它所供给的那一步来解析，因此它无法影响
`clap` 自己读到的值。

布尔值接受 `1`、`true`、`yes`、`on`、`0`、`false`、`no`、`off`。拼错是错误而不是
`false`——它把守的两件事（轮换数据库里所有密钥、提供控制台）都属于「出事之前没人
会发现它没生效」的那一类。

### 配置文件

文件用的是 `AppConfig` 自己的字段名，也就是 `snake_case`。未知键是错误而不是静默
忽略，所以拼写错误会在启动时被报出来。

```toml
host = "0.0.0.0"
port = 8787
data_dir = "/var/lib/gproxy"
public_base_url = "https://gproxy.example.com"
cors_origins = ["https://console.example.com"]
trusted_proxies = ["10.0.0.0/8"]
session_ttl_secs = 2592000

[store]
kind = "sqlite"
path = "gproxy.db"
# kind = "url"
# dsn = "postgres://gproxy@db/gproxy"

[cache]
kind = "memory"
# kind = "redis"
# url = "redis://cache:6379"
# namespace = "prod"

[master_key]
rotate = false

[master_key.key]
kind = "hex"
value = "0000000000000000000000000000000000000000000000000000000000000000"

[file_storage]
kind = "fs"
root = "files"
# kind = "s3"
# bucket = "gproxy"
# region = "auto"
# endpoint = "https://…"

[console]
enabled = true

[oauth]
access_ttl_secs = 3600
refresh_ttl_secs = 2592000
code_ttl_secs = 300
device_ttl_secs = 900
cli_client_ids = []
```

有几项没有 flag，因为它们不是容器会去覆盖的东西：`session_ttl_secs`、整个
`[oauth]` 块，以及 S3 的细节。

---

## 密钥

凭证密钥——以及保留副本的 API key 和 tokenizer 认证令牌——在静态存储时用
`GPROXY_MASTER_KEY` 下的 AES-256-GCM 密封。密封绑定到所在行自己的 id，因此把
blob 拷到另一行是解不开的。

**没有主密钥时，密钥以明文存储。** 这是一种受支持的部署，不是意外；二进制会在启动时
大声说一次，并点名那个能修好它的变量：

```
WARN upstream credential secrets are stored UNENCRYPTED: no master key is
     configured. Set GPROXY_MASTER_KEY to 32 bytes as 64 hex characters or
     base64 …
```

### 轮换

换密钥不是改配置：每一个密封 blob 都必须用旧密钥解开、用新密钥重新密封，否则下一次
reload 会在第一个解不开的凭证上失败。所以轮换是一次真实的操作，在启动时执行：

```sh
# 1. 停掉所有实例。
# 2. 启动一个，带上已解除保险的轮换开关。
GPROXY_MASTER_KEY=<旧> \
GPROXY_MASTER_KEY_NEXT=<新> \
GPROXY_MASTER_KEY_ROTATE=true \
  gproxy serve

# 3. 它会以 WARN 记录：
#    master key rotated; copy GPROXY_MASTER_KEY_NEXT to GPROXY_MASTER_KEY,
#    then clear GPROXY_MASTER_KEY_NEXT and GPROXY_MASTER_KEY_ROTATE

# 4. 把新密钥提升为 GPROXY_MASTER_KEY，清掉另外两个。
GPROXY_MASTER_KEY=<新> gproxy serve
```

三条性质让它可以放心执行：

- **一个事务。** 所有 `UPDATE` 和 revision 自增一起提交，所以失败会让数据库完整地
  留在旧密钥上，直接重试即可。绝不会出现「轮换到一半」的数据库需要先搞清楚它的形状。
- **先全部解开，再写任何东西。** 一把只能解开大部分行的密钥会在第一次写之前中止。
- **不跳过任何一行。** 解不开的 blob 是错误，绝不是「留在原地」的行——一旦运维提升了
  新密钥，被跳过的行对**两把**密钥都解不开。

设计上还有两条随之而来的规则：

- **轮换时只跑一个实例。** 数据库里没有「当前是哪把密钥」的持久记录，所以仍持有旧密钥
  的对端会看到 revision 自增、reload，然后什么都解不开。
- **执行轮换的进程此后用新密钥服务**，因为数据库现在就是新密钥的。忘掉第 4 步意味着
  下次启动什么都打不开——而且是在装配阶段大声失败，不是悄悄地。

`GPROXY_MASTER_KEY_NEXT` 配合未设置的 `GPROXY_MASTER_KEY`，就是「采用加密」的路径：
它会把一个一直明文运行的数据库密封起来。

设置了 `GPROXY_MASTER_KEY_NEXT` 却**没有** `GPROXY_MASTER_KEY_ROTATE` 时什么都不会
发生，并且会说出来——这样一把在部署里躺了一个月的 next key 不会被误当成已经轮换过。

---

## Bootstrap

全新的数据库没有入口：控制台需要用户，管理 API 需要 key。所以首次启动建一个管理员、
铸一把网关 API key，并向标准输出打印一次：

```
GPROXY first-run administrator (shown once)
  user:     admin
  password: eUGXB4mnDhWmoYrDafattTFVjoR3SIyE
  api key:  sk-9uzmySzNRy0uT0ERTTrKNbfv5DeMkoXseGb0dULT19s
Save these before closing this terminal; they are not stored in a form
this instance can show you again.
```

- 触发条件是 **`users` 表为空**。只要存在任何一个用户，就说明这个实例已经被设置过，
  什么都不碰：不重置密码、不铸 key、不改任何行。一个重启了「环境里仍带着
  `GPROXY_ADMIN_PASSWORD` 的容器」的运维，并不是在要求重置密码。
- **只打印运维还不知道的东西。** 提供了 `GPROXY_ADMIN_PASSWORD` 就使用它但不回显；
  提供了 `GPROXY_BOOTSTRAP_ADMIN_API_KEY` 就铸造那把确切的 key，而不是新的。
- 输出走 **stdout 的 `println!`，绝不进日志**，密钥因此不会落进 journal 或把 journal
  运走的东西里。日志行出于同样的理由走 stderr——这也正是 `export --out -` 干净的原因。
- 每一次写入都经过 `gproxy-app` 自己的 `Operations` 家族，所以密码由产品的规则校验并
  用 argon2 哈希，key 由产品的函数计算摘要。v3 里 bootstrap key 是用手写 SQL、按一个
  管理 API 并不用来查找它的摘要写进去的，于是带 `sk-` 前缀的 bootstrap key 每个请求
  都回 `401`。解法不是换一个更好的摘要——而是只保留一个。

---

## 控制台

`/console` 由编译进二进制的产物提供，或由 `GPROXY_CONSOLE_PATH` 指定的目录提供。
**源码检出不内嵌任何东西**，这是预期状态：`cargo run` 出来的二进制里没有控制台，
所以每个控制台路径都回 404，而不是一个看起来像应用坏了的白页。启动日志会说明这一点。

产物放在 `gproxy-host-axum/assets/web`，由 release 构建在 `cargo build` 之前从控制台
自己的 `dist` 填入。本 crate 不参与内嵌——接缝是
[`gproxy_host_axum::console`](../gproxy-host-axum/src/console.rs) 里的 `rust-embed`，
已经就位，等控制台落进去。

---

## Cargo features

| Feature | 默认 | 提供 |
|---|---|---|
| `channels` | ✓ | 本仓库实现的全部渠道。想只带上部署实际会用的上游，就逐个点名（`codex`、`kiro`、`openai`……） |
| `memory` | ✓ | 进程内缓存 |
| `fs` | ✓ | 本地文件存储 |
| `bundled-vocabulary` | ✓ | 兜底的分词词表 |
| `postgres` | | PostgreSQL 驱动 |
| `mysql` | | MySQL 驱动 |
| `redis` | | 多实例部署所需的共享缓存 |
| `s3` | | S3 兼容的文件存储 |

SQLite 始终编译在内。本次构建没有的后端会在启动时被拒绝，并指出能提供它的 feature
名字，而不是等到第一个请求才失败。

---

## 跑多个实例

有三件事不一样：

1. **`GPROXY_REDIS_URL`。** 默认缓存是进程内的，两个实例互相看不见对方的失效通知，
   限流也会各算各的。
2. **把 `gproxy migrate` 作为独立步骤**，单写者，在任何实例启动之前跑。
3. **轮换时只跑一个实例**，见上文。
