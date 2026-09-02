---
title: "配置"
description: "命令行参数、GPROXY_* 环境变量、.env 分层、原生宿主专用与构建期变量，以及存放在数据库中的实例设置"
---

GPROXY 在启动时一次性读取进程配置，来源是命令行参数、环境变量和 `.env`
文件。除 `.env` 之外没有别的配置文件：v3 不读取 TOML。运行期间会变化的
一切——Provider、凭证、路由、规则、定价、身份，以及本页末尾的实例设置——
都保存在数据库中，通过控制台或 admin API 编辑。

`gproxy --help` 与环境变量列表由同一份声明生成，两者不会漂移。每个参数都
有对应的 `GPROXY_*` 环境变量；下表同时列出两者。

## 优先级顺序

一个值取自最先设置它的来源：

1. 命令行参数；
2. 进程环境变量；
3. 工作目录下的 `./.env`；
4. `<data-dir>/.env`，仅当它与 `./.env` 不是同一个文件时才读取；
5. 内置默认值。

`GPROXY_DATA_DIR` 本身只从前三个来源解析，因为必须先知道数据目录才能读取
其中的 `.env`。相对路径的数据目录相对于工作目录解析，启动时不存在则创建。

## `.env` 格式

```bash
# <data-dir>/.env
GPROXY_HOST=0.0.0.0
GPROXY_PORT=8787
GPROXY_PERSISTENCE=postgres
GPROXY_DSN=postgres://gproxy:<password>@db.internal:5432/gproxy
GPROXY_MASTER_KEY=<standard-base64-32-bytes>
```

- 每行一个 `KEY=value`。键和值都会去除首尾空白；引号不会被去掉，所以不要
  给值加引号。
- `#` 在一行的任何位置都开始注释，因此值中不能包含 `#`。这类值请放到真实
  环境变量中。
- 非空行缺少 `=` 是启动错误，报错信息会指出文件和行号。
- 只读取以 `GPROXY_` 开头的键，以及 `UPSTASH_URL` 和 `UPSTASH_TOKEN`。共享
  部署 `.env` 中的其他键会被忽略，不会进入进程。

## 监听与数据目录

| 变量 | 参数 | 默认值 | 含义 |
| --- | --- | --- | --- |
| `GPROXY_HOST` | `--host <ADDR>` | `127.0.0.1` | 绑定的网络接口。`host:port` 必须能解析为 socket 地址，因此 IPv6 地址需要方括号：`[::1]`。 |
| `GPROXY_PORT` | `--port <PORT>` | `8787` | TCP 端口。 |
| `GPROXY_DATA_DIR` | `--data-dir <PATH>` | `./data` | 存放 SQLite 文件 `gproxy.db`、可选的 `.env`、自更新暂存目录（`.update/`）和登录启动标记文件。 |

容器镜像预设 `GPROXY_HOST=0.0.0.0`、`GPROXY_DATA_DIR=/var/lib/gproxy` 和
`GPROXY_PERSISTENCE=sqlite`；见[容器部署](/zh-cn/deployment/docker/)。

## 持久化

| 变量 | 参数 | 默认值 | 含义 |
| --- | --- | --- | --- |
| `GPROXY_PERSISTENCE` | `--persistence <BACKEND>` | `sqlite` | `sqlite`、`libsql`、`postgres` 或 `mysql`（不区分大小写）。 |
| `GPROXY_DSN` | `--dsn <DSN>` | 无 | `postgres` 或 `mysql` 的连接串；这两种后端必填。 |
| `GPROXY_LIBSQL_URL` | `--libsql-url <URL>` | 无 | libSQL 服务器的绝对 `http(s)` URL；`libsql` 后端必填。 |
| `GPROXY_LIBSQL_AUTH_TOKEN` | `--libsql-auth-token <TOKEN>` | 无 | 该服务器的 Bearer Token；`libsql` 后端必填且不能为空。 |

DSN 格式与各后端行为见[存储与缓存后端](/zh-cn/reference/database/)。

## 缓存

| 变量 | 参数 | 默认值 | 含义 |
| --- | --- | --- | --- |
| `GPROXY_REDIS_URL` | `--redis-url <URL>` | 无 | Redis 共享缓存。与 Upstash 同时设置时 Redis 优先。 |
| `UPSTASH_URL` | `--upstash-url <URL>` | 无 | Upstash Redis REST 端点；绝对 `http(s)` URL。 |
| `UPSTASH_TOKEN` | `--upstash-token <TOKEN>` | 无 | Upstash REST Token。两个 `UPSTASH_*` 只设置其一是启动错误。 |

以上都未设置时，缓存为进程内缓存；持久化为 `libsql` 时则是 libSQL 数据库中
的一张表。配额、限流、准入状态、刷新租约和亲和绑定都存放在缓存里，因此多
实例部署必须使用 Redis 或 Upstash。

## 密钥与加密存储

| 变量 | 参数 | 默认值 | 含义 |
| --- | --- | --- | --- |
| `GPROXY_MASTER_KEY` | `--master-key <BASE64>` | 未设置 | 标准 base64，解码后恰为 32 字节。设置后，凭证、用户 API 密钥和 Hugging Face Token 以 AES-256-GCM 信封加密密封存储。未设置表示明文存储。 |
| `GPROXY_MASTER_KEY_NEXT` | `--master-key-next <BASE64>` | 未设置 | 要轮换到的新密钥。空值表示轮换回明文。未武装轮换时忽略并打印警告。 |
| `GPROXY_MASTER_KEY_ROTATE` | `--master-key-rotate <BOOL>` | 关 | 武装轮换：`1`、`true`、`yes`、`on`；或 `0`、`false`、`no`、`off`、空。其他值是启动错误。 |

存储会记录密封所用密钥的 SHA-256 指纹。已密封的存储用不同密钥或不带密钥
打开时，启动会拒绝；明文存储在设置了密钥的情况下打开，启动同样拒绝。无论
哪个方向的切换都必须经过轮换，不会静默重新加密。

轮换步骤：

1. 保持 `GPROXY_MASTER_KEY` 为当前密钥（明文存储则保持未设置）。将
   `GPROXY_MASTER_KEY_NEXT` 设为新密钥，或设为空字符串以回到明文。设置
   `GPROXY_MASTER_KEY_ROTATE=on`。
2. 启动 GPROXY 一次。它用当前密钥打开每个已存储的秘密，用新密钥重新密封，
   并在一次写入中替换秘密清单和指纹。日志末尾会有一条警告提示完成轮换。
3. 停止 GPROXY。把 `GPROXY_MASTER_KEY` 设为新密钥（或取消设置），清除
   `GPROXY_MASTER_KEY_NEXT` 和 `GPROXY_MASTER_KEY_ROTATE`，再次启动。

设置了 `GPROXY_MASTER_KEY_ROTATE=on` 却未设置 `GPROXY_MASTER_KEY_NEXT` 是
启动错误。

## 网络与限制

| 变量 | 参数 | 默认值 | 含义 |
| --- | --- | --- | --- |
| `GPROXY_UPSTREAM_PROXY_URL` | `--upstream-proxy-url <URL>` | 无 | 默认出站代理。优先级：凭证代理，然后 Provider 代理，最后此值。它会覆盖实例设置中的 `proxy`。除非开启 `inherit_system_proxy`，环境中的 `HTTP_PROXY`/`HTTPS_PROXY` 会被忽略。自更新和公告拉取也使用它。 |
| `GPROXY_TRUSTED_PROXIES` | `--trusted-proxy <IP>` | 空 | 逗号分隔的 IP。仅当对端是回环地址或列表中的地址时，才采信 `X-Forwarded-For`（第一项）和 `X-Real-IP`。 |
| `GPROXY_CORS_ORIGINS` | `--cors-origin <ORIGIN>` | 空 | 逗号分隔的精确 Origin。为空则不发送 CORS 头（仅同源）。允许的方法 `GET, POST, PATCH, DELETE, OPTIONS`；允许的头 `authorization, content-type, x-api-key`；允许携带凭据。 |
| `GPROXY_MAX_ATTEMPTS` | `--max-attempts <COUNT>` | `6` | 单个请求上游尝试次数的上限。路由自身的 `max_attempts` 受它约束。必须为正数。 |
| `GPROXY_MAX_IN_FLIGHT` | `--max-in-flight <COUNT>` | `1024` | 监听器同时服务的请求数。每个请求（包括控制台和 admin API）都占用一个许可；超出的请求排队等待。必须为正数。 |
| `GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT` | `--file-upload-max-in-flight <COUNT>` | 未设置 | 本进程 `POST /v1/files` 和 `POST /upload/v1beta/files` 的上传并发数。`0` 表示不限制。设置后覆盖控制台中的同名设置。 |
| `GPROXY_INSTANCE_ID` | `--instance-id <ID>` | `0` | 原生请求 ID 的首段（`<instance>-<启动前缀>-<序号>`）。多实例部署请为每个实例设置不同的值。 |
| `GPROXY_LOG_FORMAT` | `--log-format <FORMAT>` | `text` | `text` 或 `json`（按行分隔）。 |
| `RUST_LOG` | — | `info` | 原生日志的标准 `tracing` 过滤器。只从进程环境变量读取。 |

请求体上限为 100 MiB。`Content-Encoding: zstd` 的请求体在入口解码；其他编码
返回 415。两者都不可配置。

## 首次启动引导

以下变量作用于全新存储，即尚无管理员的存储。未设置 `GPROXY_ADMIN_PASSWORD`
时，首次访问 `/admin` 会显示创建管理员的初始化页面。

| 变量 | 参数 | 默认值 | 含义 |
| --- | --- | --- | --- |
| `GPROXY_ADMIN_USER` | `--admin-user <USER>` | `admin` | 引导时使用的管理员用户名。 |
| `GPROXY_ADMIN_PASSWORD` | `--admin-password <PASSWORD>` | 未设置 | 全新存储：用此密码创建管理员并生成一个 API 密钥。已有存储：若该用户存在则重置其密码；其他账户永不改动。 |
| `GPROXY_BOOTSTRAP_ADMIN_API_KEY` | `--bootstrap-admin-api-key <KEY>` | 自动生成 | 仅对全新存储、且仅在设置了 `GPROXY_ADMIN_PASSWORD` 时生效：管理员的第一个 API 密钥。未设置则随机生成。无论哪种方式，密钥都会像其他密钥一样密封存储，只能通过控制台的"显示"操作查看。空白值是错误。 |
| `GPROXY_BOOTSTRAP_CHANNELS` | `--bootstrap-channel <CHANNEL>` | 空 | 逗号分隔的通道 ID。仅对全新存储、且设置了 `GPROXY_ADMIN_PASSWORD` 时生效：为每个通道创建一个同名的已启用 Provider，并附上该通道的默认规则集。未知 ID 是启动错误。 |

全新存储上设置了引导密钥或通道却未设置 `GPROXY_ADMIN_PASSWORD` 是启动
错误。全新存储还会加载内置的全局价格目录；见
[价格与分层](/zh-cn/reference/pricing/)。

## 原生宿主专用变量

原生二进制只从进程环境变量读取这些值——不读 `.env`——也没有对应的命令行
参数。

| 变量 | 默认值 | 含义 |
| --- | --- | --- |
| `GPROXY_AUTOSTART` | `on` | 按用户登录启动项（Linux `.desktop`、macOS LaunchAgent、Windows Run 键）的首次运行默认值。只读取一次，直到 `<data-dir>/.autostart-initialized` 存在；之后由控制台的"登录时启动"开关管理。接受 `on`/`off`、`true`/`false`、`1`/`0`、`yes`/`no`、`enable(d)`/`disable(d)`。保存的启动命令会重复当前参数，并在环境中存在 `GPROXY_MASTER_KEY` 时追加 `--master-key`。 |
| `GPROXY_UPDATE_CHANNEL_SERVE` | 构建通道 | 优先级最高的更新通道：`releases`（也接受 `release`、`stable`）、`staging` 或 `dev`（也接受 `development`）。 |
| `GPROXY_UPDATE_CHANNEL` | 构建通道 | 取值相同；`GPROXY_UPDATE_CHANNEL_SERVE` 未设置时生效。完整优先级：`_SERVE`，然后 `GPROXY_UPDATE_CHANNEL`，然后控制台的更新通道设置，最后是构建通道。名称无效时更新请求返回 400。 |
| `GPROXY_UPDATE_SERVE` | GitHub release URL | 覆盖所有通道的 manifest URL。默认：`dev` 和 `staging` 读取 `releases/download/<channel>/manifest.json`，`releases` 读取 `releases/latest/download/manifest.json`，均来自 GPROXY 仓库。 |
| `GPROXY_UPDATE_RESTART` | `none` | 应用更新或回滚之后的动作：`none`（由你重启）、`supervisor`（250 ms 后以退出码 42 退出，交给守护进程重启）、`re-exec`（也接受 `reexec`；Unix 上以相同参数 exec 新二进制，其他平台退出码 42）。值无效会禁用自更新，其端点返回 503。 |

当 manifest 要求的最低数据版本高于本二进制的 schema 版本、版本号无效，或
回滚时不存在 `<exe>.prev`，更新会以 409 拒绝。

## 构建期标识

这些是编译期输入（`option_env!`），在 `cargo build` 的环境中设置，不是运行
时配置。

| 变量 | 默认值 | 含义 |
| --- | --- | --- |
| `GPROXY_UPDATE_PUBKEY` | 无 | 标准 base64 的 Ed25519 公钥，用于校验签名的更新 manifest。没有它的构建，每次更新检查都会以签名错误失败。 |
| `GPROXY_BUILD_VERSION` | `CARGO_PKG_VERSION` | `--version` 报告的版本，也用于与 manifest 比较。 |
| `GPROXY_BUILD_CHANNEL` | `development` | 构建所属的更新通道。`development` 解析为 `dev`。 |
| `GPROXY_BUILD_HASH` | git 短哈希 | `build.rs` 用 `git rev-parse --short=12 HEAD` 填充；没有仓库时为 `unknown`。 |
| `GPROXY_INSTALLATION_KIND` | `source` | `--version` 报告的安装来源标签；安装程序会设置自己的值。 |

```text
$ gproxy --version
gproxy 3.0.0-alpha.0 (channel development, build 4054fe4f94ea, installation source)
```

## Edge 绑定

wasm 宿主没有命令行，也不读 `.env`。平台包装层把同名绑定传入 edge 配置：
`GPROXY_LIBSQL_URL` 与 `GPROXY_LIBSQL_AUTH_TOKEN`（必填），
`GPROXY_MASTER_KEY`、`GPROXY_MASTER_KEY_NEXT` 与 `GPROXY_MASTER_KEY_ROTATE`
（可选），以及 `UPSTASH_URL` 与 `UPSTASH_TOKEN`（可选，须同时设置）。持久化
始终是 libSQL；未设置 Upstash 时缓存为 libSQL 表。监听、数据目录、引导和原
生宿主各行不适用。见 [Edge Wasm](/zh-cn/deployment/edge/)。

## 实例设置

运行时设置保存在 `settings` 表中，在控制台 → 设置里编辑
（`GET`/`PATCH /admin/api/instance-settings` 和 `/admin/api/log-settings`），
无需重启即生效。

| 键 | 控制台标签 | 默认值 | 含义 |
| --- | --- | --- | --- |
| `instance_name` | 实例名称 | `default` | 在日志和遥测中显示。 |
| `proxy` | 默认上游代理 | 无 | 在凭证和 Provider 代理之后使用；`GPROXY_UPSTREAM_PROXY_URL` 会覆盖它。 |
| `inherit_system_proxy` | 继承系统代理 | 关 | 没有显式代理时采用 `HTTP_PROXY`/`HTTPS_PROXY`。仅原生宿主。 |
| `enable_usage` | 用量记录 | 开 | 结算后持久化用量记录。关闭时准入和配额核算照常进行。 |
| `enable_tokenizer_vocabs` | 使用词表 | 开 | 用真实词表统计 token；关闭则回退到字符估算。仅原生宿主。 |
| `enable_tokenizer_download` | 自动下载词表 | 关 | 统计时自动从 Hugging Face 拉取未缓存的词表。 |
| `default_tokenizer_vocab` | 默认词表 | 无 | 模型未匹配 Provider `tokenizer_map` 中任何模式时使用。 |
| `file_upload_max_in_flight` | 文件上传并发数 | `0` | `0` 表示不限制；环境变量覆盖优先。 |
| `retention_days` | 保留天数 | 未设置 | 用量记录、请求日志和线路日志的保留期限。未设置时按 36,500 天处理。 |
| `max_database_size_mb` | 数据库大小上限（MiB） | 未设置 | 超过上限时删除最旧的请求日志和线路日志；用量记录不会因大小被删除。未设置时按 1,024 MiB 处理。 |
| `enable_downstream_log`、`enable_downstream_log_body` | 下游元数据 / 正文 | — | 记录调用方请求与响应的元数据，可选记录正文。 |
| `enable_upstream_log`、`enable_upstream_log_body` | 上游元数据 / 正文 | — | 记录每次上游尝试，可选记录正文。 |
| `disable_log_redaction` | 停用日志脱敏 | 关 | 以明文存储捕获的头和正文。脱敏默认开启。 |
| `traffic_blacklist` | 全局元数据黑名单 | 内置列表 | 在内置列表之上，实例范围内额外移除的请求头、响应头和 query 参数名。 |
| `update_channel`、`enable_auto_update_check` | 更新 | 构建通道 | 控制台对更新通道和自动检查的偏好。 |

Hugging Face Token 密封存放在单独的表（`tokenizer_auth`）中，不在
`settings` 里。同一控制台页面上的登录启动和更新操作由原生宿主提供，不经
过数据库。

## 关闭

原生二进制等待 `Ctrl-C`（SIGINT）。收到信号后停止接受连接，让进行中的
请求和流式响应完成后退出；没有排空超时。不处理其他信号：SIGTERM 会按默认
动作立即结束进程。守护进程和容器请用 SIGINT 停止 GPROXY——例如
`docker kill --signal SIGINT <container>`——因为发布的镜像没有设置
`STOPSIGNAL`。
