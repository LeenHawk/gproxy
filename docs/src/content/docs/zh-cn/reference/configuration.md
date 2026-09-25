---
title: "配置"
description: 五个配置来源及其顺序、全部 25 个 GPROXY_* 变量、TOML 文件、主密钥轮换、bootstrap，以及运行期 settings 行。
---

GPROXY **只在启动时读一次**进程配置。入口点之下没有任何模块读环境变量。

一切在进程运行期间会变的东西——Provider、凭证、路由、改写规则、价格、身份，以及本页末尾
的 settings 行——都在数据库里。

## 五个来源

从强到弱：

1. **命令行**——`--port 9000`
2. **真实环境**——`GPROXY_PORT=9000`
3. **`.env`**，加载时不覆盖环境已经设过的任何东西
4. **`--config` 点名的 TOML 文件**
5. **内置默认值**

把文件放在环境*下面*是要紧的那个选择。文件是一套部署签进版本库的意图；环境是某一台主机或
某一个容器偏离它的方式。如果文件赢了，compose 文件里的 `GPROXY_PORT` 就会静默地什么也不做。

每个配置 flag 都是全局的，因此 `gproxy --port 9000 serve` 和 `gproxy serve --port 9000`
是同一次调用。

## 环境变量

每个值都是一个带着自己变量名的 flag，因此 `gproxy --help` 会把变量印在它所遮蔽的 flag 旁边，
这张表也就没法从程序上漂走。v3 就有的名字含义不变：升级不是重新部署。

| 变量 | Flag | 默认 | 是什么 |
| --- | --- | --- | --- |
| `GPROXY_CONFIG` | `--config`、`-c` | — | 装任意配置字段的 TOML 文件 |
| `GPROXY_HOST` | `--host` | `127.0.0.1` | 监听地址（一个 IP，不是主机名） |
| `GPROXY_PORT` | `--port`、`-p` | `8787` | 监听端口 |
| `GPROXY_DATA_DIR` | `--data-dir` | `data` | 相对路径相对它解析的根 |
| `GPROXY_PERSISTENCE` | `--persistence` | `sqlite` | `sqlite`、`postgres` 或 `mysql` |
| `GPROXY_DSN` | `--dsn` | — | 连接串；没有 `--persistence` 时由它自己点明后端 |
| `GPROXY_REDIS_URL` | `--redis-url` | — | 共享 cache；**多实例必需** |
| `GPROXY_MASTER_KEY` | `--master-key` | — | 32 字节，64 个十六进制字符或 base64。不设则明文存放 |
| `GPROXY_MASTER_KEY_NEXT` | `--master-key-next` | — | 要重新密封到的那把钥匙 |
| `GPROXY_MASTER_KEY_ROTATE` | `--master-key-rotate` | `false` | 启动时执行轮换 |
| `GPROXY_PUBLIC_BASE_URL` | `--public-base-url` | — | 对外源，用于发布链接和 OAuth issuer 标识 |
| `GPROXY_CORS_ORIGINS` | `--cors-origin` | — | 逗号分隔的浏览器 origin；空表示仅同源 |
| `GPROXY_TRUSTED_PROXIES` | `--trusted-proxy` | — | 逗号分隔、其 `x-forwarded-*` 可被相信的对端；空表示谁都不信 |
| `GPROXY_FILE_STORAGE_DIR` | `--file-storage-dir` | — | 发布 body 与词表的本地目录 |
| `GPROXY_AUDIT_ENABLED` | `--audit-enabled` | `true` | 记录管理及 OAuth 审计；设为 `false` 关闭新增记录 |
| `GPROXY_CONSOLE` | `--console` | `true` | 提供 console |
| `GPROXY_CONSOLE_PATH` | `--console-path` | — | 从这个目录而不是内嵌 bundle 提供 console |
| `GPROXY_INSTANCE_ID` | `--instance-id` | 随机 | 本进程的稳定名字 |
| `GPROXY_LOG_FORMAT` | `--log-format` | `text` | `text` 或 `json` |
| `GPROXY_LOG_FILTER` | `--log-filter` | `RUST_LOG`，再 `info` | tracing 过滤器 |
| `GPROXY_ADMIN_USER` | `--admin-user` / `--user` | `admin` | 第一个管理员的名字 |
| `GPROXY_ADMIN_PASSWORD` | `--admin-password` / `--password` | 生成 | 他的密码 |
| `GPROXY_BOOTSTRAP_ADMIN_API_KEY` | `--admin-api-key` / `--api-key` | 生成 | 要为他签发的那把确切 API key |
| `GPROXY_IMPORT_SOURCE_MASTER_KEY` | `--source-master-key` | — | 仅 `import`：源实例的钥匙 |
| `GPROXY_ENV_FILE` | — | `.env` | 加载哪个 `.env` |

`GPROXY_ENV_FILE` 刻意没有 flag：flag 得由它所喂养的那一步来解析，因此它影响不了解析器
自己读到的值。

布尔值接受 `1`、`true`、`yes`、`on`、`0`、`false`、`no`、`off`。**拼错是错误而不是
`false`**。

关闭审计：设置 `GPROXY_AUDIT_ENABLED=false`、传入 `--audit-enabled false`，或在 TOML 顶层设置 `audit_enabled = false`，然后重启。历史审计仍可查询；请求日志、用量统计不受影响。防止重复导入的迁移完成标记仍会保存。

## 命令

| 命令 | 做什么 |
| --- | --- |
| `gproxy serve` | 按需轮换主密钥、实例是新的就 bootstrap、绑定、服务。**默认命令**——不带子命令的 `gproxy` 就是 `gproxy serve`。 |
| `gproxy migrate` | 创建或增量同步表结构，然后退出。 |
| `gproxy bootstrap admin` | 创建第一个管理员。幂等。 |
| `gproxy export --out <PATH>` | 把本实例的配置写成一份 JSON 文档。 |
| `gproxy import --in <PATH>` | 把这样一份文档回放进本实例。 |

`serve` 在 `SIGINT` 或 `SIGTERM` 上停止，先把在途请求排空。**没有关停超时**：一次流式
补全跑上几分钟是合法的，而想要截止时间的 supervisor 自己有。

绑定发生在表结构工作**之后**，因此一个还在迁移的进程直接拒绝连接，而不是把它们收进一个
没人应答的积压队列。

## 配置文件

文件讲的是配置类型自己的字段名，`snake_case`。**未知键是错误**，因此拼写错误在启动时就被
报出来，而不是被静默忽略。

```toml
host = "0.0.0.0"
port = 8787
audit_enabled = true
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

有几个字段没有 flag，因为它们不是容器会去覆盖的东西：`session_ttl_secs`、整个 `[oauth]`
块，以及 S3 细节。

这就是 Workers 宿主在 `GPROXY_CONFIG` 里读作 JSON、桌面宿主在数据目录里读作 `gproxy.toml`
的**同一份文档**，所以没有第二套 schema 要维持同步。

## 静态密钥

凭证密钥、被保留的 API key 和 tokenizer 源令牌，都用主密钥以 AES-256-GCM 密封。
**密封绑定到该行自己的 id**，所以一份密文被拷到另一行上打不开。

没有主密钥时密钥以明文存放。这是一种受支持的部署形态，不是意外，而且二进制在启动时会
大声说一次，并点名那个能修好它的变量：

```text
WARN gproxy::serve: upstream credential secrets are stored UNENCRYPTED: no
     master key is configured. Set GPROXY_MASTER_KEY to 32 bytes as 64 hex
     characters or base64 — and, on a database that already holds secrets, set
     GPROXY_MASTER_KEY_NEXT with GPROXY_MASTER_KEY_ROTATE to seal what is
     already there.
```

用 `openssl rand -hex 32` 生成一把，正好 64 个十六进制字符。32 字节的 base64 是 43 或 44
个字符，所以两种编码不会混淆，格式是嗅探出来的而不是配置出来的。

### 主密钥轮换

换钥匙不是改一行配置。每一份密文都必须用旧钥匙打开、再用新钥匙封上，否则下一次重载会在
它打不开的第一把凭证上失败。所以轮换是一次真正的操作，在启动时执行：

```sh
# 1. 停掉每一个实例。
# 2. 带着轮换开关启动一个。
GPROXY_MASTER_KEY=<old> \
GPROXY_MASTER_KEY_NEXT=<new> \
GPROXY_MASTER_KEY_ROTATE=true \
  gproxy serve

# 3. 它会在 WARN 级别记录：
#    master key rotated; copy GPROXY_MASTER_KEY_NEXT to GPROXY_MASTER_KEY,
#    then clear GPROXY_MASTER_KEY_NEXT and GPROXY_MASTER_KEY_ROTATE

# 4. 把新钥匙提升上来，清掉另外两个。
GPROXY_MASTER_KEY=<new> gproxy serve
```

三条性质让它敢跑：

- **一个事务。** 每一次更新和 revision 自增一起提交，所以失败只会让数据库整体停在旧钥匙
  上，重试即可。绝不会出现一个半轮换的数据库要先搞清楚它是什么形状。
- **写之前先全部打开。** 一把能解开大多数行的钥匙，会在第一次写入之前就中止。
- **什么都不跳过。** 打不开的密文是错误，绝不是一行被留下——被跳过的行在运维者提升新钥匙
  之后，就是**两把**钥匙都打不开了。

由此得出两条规则：

- **轮换时只跑一个实例。** 数据库现在在哪把钥匙上没有持久记录，所以一个还拿着旧钥匙的
  同伴会看到 revision 自增、重载，然后什么都打不开。
- **执行轮换的进程此后一直用新钥匙服务**，因为数据库现在就装着那个。忘了第 4 步意味着
  下次启动什么都打不开——大声地、在装配时，而不是静默地。

`GPROXY_MASTER_KEY_NEXT` 配上没设的 `GPROXY_MASTER_KEY` 是**采纳路径**：它给一个本来在跑
明文的数据库加封。

设了 `GPROXY_MASTER_KEY_NEXT` 却*没有* `GPROXY_MASTER_KEY_ROTATE` 时什么也不做，并且会
这么说——这样一把在部署里躺了一个月的"下一把钥匙"，不会被误当成一次已经发生过的轮换。

## Bootstrap

一个全新的数据库没有入口，所以首次启动创建一个管理员、签发一把网关 API key，并把两者
只打印一次到标准输出。

- **触发条件是 users 表为空。** 只要有任何用户，就说明这个实例已经被设置过：不重置密码、
  不签发 key、不改任何一行。一个重启还带着 `GPROXY_ADMIN_PASSWORD` 的容器的运维者，并不
  是在要求重置密码。
- **只打印运维者还不知道的东西。** 提供了 `GPROXY_ADMIN_PASSWORD` 就使用它但不回显；
  提供了 `GPROXY_BOOTSTRAP_ADMIN_API_KEY` 就签发那把确切的 key。
- **输出走 stdout，绝不走日志**，这样密钥不会落进 journal 或者把它运走的任何东西。
- **每一次写入都走产品自己的操作家族**，所以密码由产品的规则校验和哈希，key 由产品的函数
  做摘要。v3 的 bootstrap key 是用手写 SQL 按一个管理 API 并不用它来查的摘要写下去的，
  于是一把 `sk-` 前缀的 bootstrap key 在每个请求上都回 `401`。修法不是换一个更好的摘要
  ——而是只有一个。

## 搬运一份配置

```sh
gproxy export --out config.json --include-secrets
gproxy import --in  config.json --mode merge --source-master-key '…'
```

`-` 表示标准输出或标准输入。文档就是管理 DTO 本身，所以导出说的正是一次列表会说的：

```json
{"formatVersion":5,"exportedAtMs":1789981723118,"secretsOmitted":true,
 "secrets":[],"data":{"connectionProfiles":[],"providers":[…],"credentials":[…],
 "models":[],"providerModels":[],"routes":[],"routeMembers":[],
 "operationRules":[],"operationEndpoints":[],
 "rewriteRuleSets":[],"rewriteRules":[],"providerRewriteRuleSets":[],
 "quotas":[],"priceRules":[],"priceRates":[],"priceTiers":[],"settings":null}}
```

`data` 按回放顺序排列：没有任何一行出现在它所指向的那一行之前。

**身份不走**——用户、key、组织、团队、权限、订阅和 OAuth 客户端属于产品层——所以被导入的
实例仍然需要它自己的 bootstrap。用量和 capture 也不走：拷贝它们等于伪造目的端从未有过的
历史。

`--mode merge` 写文档点名的东西，其余不动。`--mode replace` 还会删掉文档没提到的、属于
已导出种类的行，先子后父，并且绝不碰身份、用量或 capture 表。一次导入是**整份文档一个
revision 提交**，因此任何一处被拒绝的文档什么也不会留下。

带 `--include-secrets` 时密文以 base64 随行，从不被打开、也从不是明文——这份文档因此与
数据库文件同等敏感。回来的时候：

| 导入方有什么 | 会发生什么 |
| --- | --- |
| `--source-master-key` | 每个密钥被打开并用本实例的钥匙重新密封 |
| 与源相同的钥匙 | 密文原样存放，本来就能打开 |
| 都没有 | 这把凭证被跳过、计数并告警 |

这条规则存在，是因为引擎在装配快照时会打开每一把凭证的密钥：一把导入后打不开的凭证不会
只是坏掉它自己的调用，它会坏掉**整个实例此后每一次重载**。

## settings 行

控制台的「系统 → 全局设置」直接修改该行；只提交已修改字段，不覆盖其他设置。
实例名称显示在侧栏及网页标题，版本和完整 Git hash 可从 `/info` 读取。

- `corsOrigins`、`trustedProxies` 按当前快照读取；启动值只在新建 settings 行时初始化。
- 原生 `gproxy serve` 跟随数据库更新进程日志级别／格式、更新通道和自动检查开关。
- `enableTokenizerVocabs=false` 停用自定义词表并回退内置计数；不删除词表文件。
- `enableTokenizerDownload=false` 拒绝新下载；已有词表是否使用由前一个开关控制。
- `retentionDays` 清理超过保留期的已结束请求历史、抓包及其事件；未设置时不按时间清理。
- `maxDatabaseSizeMb` 以 MiB 限制 SQLite 历史数据占用，从最旧的已完成记录开始清理并回收空页。
  未设置或 0 不启用此限制。配置和进行中的请求不删除，因此该值不是整个数据库的硬配额。
  自动清理由原生服务每分钟执行；已结算账目、配额计数、配置及审计记录保持不变。
- `defaultFileStorageName`、`maxInFlight`、`fileUploadMaxInFlight` 已移除，不是可设置项。


运行期设置在数据库里，改了不用重启。

```sh
curl -s http://127.0.0.1:8787/admin/api/settings  -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X PATCH http://127.0.0.1:8787/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"instance":{"maxAttempts":4}}'
```

一行，两组。instance 组：

| 键 | 默认 | 含义 |
| --- | --- | --- |
| `instanceName` | `default` | 控制台名称 |
| `maxAttempts` | `6` | 一个计划上游尝试次数的硬上限；路由自己的预算被它钳住 |
| `enableSettlement`、`enableUsage` | `true` | 定价与用量行 |
| `enableTokenizerVocabs` | `true` | 用真实词表计数 |
| `enableTokenizerDownload` | `false` | 抓取未缓存的词表 |
| `retentionDays`、`maxDatabaseSizeMb` | 未设 | |
| `portalRecentRequestsEnabled` | `true` | 用户面是否展示近期请求 |
| `corsOrigins`、`trustedProxies`、`connectionProfileId`、`oauthClientAllowlist` | | 同一批策略在数据库里的那一份 |
| `configRevision` | | 只读：每个实例据以同步的 revision |

logging 组是 `enableDownstreamLog`、`enableDownstreamLogBody`、`enableUpstreamLog`、
`enableUpstreamLogBody`、`disableLogRedaction`、`enableTracing`、`logLevel`、`logFormat`
和三个黑名单。两个 body 开关默认关闭，见
[用量、日志与审计](/zh-cn/guides/observability/#capture)。

## 跑多个实例

三件事会变：

1. **`GPROXY_REDIS_URL`。** 默认 cache 是进程本地的，两个实例不会看到彼此的失效通知，
   也会各自独立地计限流。回答不了的 cache 会**拒绝**一个受限流的请求而不是放行它——静默
   地退回本地，会把一次故障变成"这个实例上所有限额都关了"。
2. **`gproxy migrate` 作为独立一步**，由单一写入者在任何实例启动前执行。
3. **轮换时只跑一个实例**，如上。

## 可信代理规则

`x-forwarded-for` 和 `x-forwarded-proto` 是 header，而 header 是对端写什么就是什么。
**只有当 socket 的对端是回环或列在 `trustedProxies` 里时**它们才被相信。来自任何其他对端
时直接忽略——不合并、不偏好、也不当兜底——而默认谁都不信。

两者都要紧，理由不同。伪造的 `x-forwarded-for` 决定运维者日志里某个请求旁边的那个地址。
伪造的 `x-forwarded-proto` 决定 **OAuth issuer 标识的 scheme**，而那是一份告诉客户端把
授权码发往哪里的发现文档。

对端未知时按不可信处理。
