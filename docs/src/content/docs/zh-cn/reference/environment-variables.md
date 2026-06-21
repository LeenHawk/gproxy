---
title: 环境变量
description: gproxy v2 支持的运行时、引导、存储、自更新和开发环境变量。
---

gproxy v2 的进程级配置来自 CLI 参数和环境变量。native 二进制使用
`clap` 解析；同一个配置项同时存在 CLI 参数和环境变量时，显式 CLI 参数优先。

启动后的大部分业务配置不再依赖环境变量。Provider、credential、model、
route、alias、权限、quota、pricing、转换规则和实例设置都存放在持久化层，
通过 console、admin API 或 JSON import/export 管理。

## Server

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `GPROXY_HOST` | `127.0.0.1` | 监听 host。IPv6 作为 CLI 参数传入时需要方括号，例如 `[::1]`。 |
| `GPROXY_PORT` | `8787` | 监听端口。 |
| `GPROXY_MAX_IN_FLIGHT` | `1024` | gateway 并发请求上限。超出的 gateway 请求会被 load-shed 为 `503`；admin 和 ops endpoint 不在这个 gateway limiter 内。 |
| `GPROXY_MAX_ATTEMPTS` | `6` | 单请求 failover candidate 尝试上限。AuthDead 后的强制刷新重试不算新的逻辑 candidate。 |
| `GPROXY_INSTANCE_ID` | `0` | 实例数字 id，用于需要按实例分区的行。多节点部署应使用不同值。 |
| `GPROXY_TRUSTED_PROXIES` | 空 | 逗号分隔的可信反向代理 IP；这些来源的 `x-forwarded-for` / `x-real-ip` 会被采信，loopback 总是可信。 |
| `GPROXY_CORS_ORIGINS` | 空 | 允许跨源访问 admin console/API 的精确 Origin 列表，逗号分隔。空值表示仅同源。 |

## 持久化与缓存

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `GPROXY_PERSISTENCE` | `db` | native 持久化后端：`db` 或 `file`。`db` 使用 SeaORM；未提供 DSN 时默认 SQLite 文件。`file` 每张表一个 JSON 文件，只适合单实例。 |
| `GPROXY_DATA_DIR` | `./data` | 数据目录。file 后端、默认 SQLite DSN、v1 迁移备份/临时文件和自更新 staging 都会用到。 |
| `GPROXY_DSN` | 自动生成 | `GPROXY_PERSISTENCE=db` 的数据库 DSN。未设置时使用 `sqlite://<absolute data_dir>/gproxy.db?mode=rwc`。 |
| `GPROXY_REDIS_URL` | 空 | Redis cache URL。只有启用 `cache-redis` feature 的 native 二进制会使用；未设置时 native 默认使用进程内 memory cache。 |
| `GPROXY_MASTER_KEY` | 空 | 标准 base64 编码的 32 字节密钥，用于打开和密封存储的 secret。缺失时进入明文 secret 模式并打印警告。该项只读环境变量，没有 CLI 参数。 |

## 上游请求与引导导入

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `GPROXY_UPSTREAM_PROXY_URL` | 空 | native 上游 provider 请求的默认代理 URL。provider 或 credential 级代理可以覆盖它；edge 部署忽略 native HTTP client 设置。 |
| `GPROXY_IMPORT_FILE` | 空 | serve 路径的一次性首启导入。设置后，如果 store 中没有 provider 且没有 user，就在 admin bootstrap 前导入该 JSON bundle；store 已有数据时跳过。 |

## Admin bootstrap

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `GPROXY_ADMIN_USER` | `admin` | 首启 bootstrap 和恢复覆盖使用的 admin 用户名。 |
| `GPROXY_ADMIN_PASSWORD` | 空 | 设置后，每次启动都会强制 upsert/reset 这个 admin 用户。密码必须满足 admin API 的同一强度策略。恢复完成后应移除。未设置且 users 表为空时，gproxy 会创建随机密码 admin 并只打印一次。 |

当前 v2 native 路径没有 `GPROXY_ADMIN_API_KEY` bootstrap 变量。用户 API key
通过 admin/portal API 生成或管理，也可以通过 JSON bundle 导入。

## 自更新

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `GPROXY_UPDATE_REPO` | 空 | admin 触发自更新和 `gproxy update` 子命令使用的 GitHub `owner/repo`。serve 路径未设置时，admin update check/apply 返回不可用。 |
| `GPROXY_UPDATE_CHANNEL_SERVE` | `releases` | serve 路径自更新 channel：`releases` 或 `staging`。 |
| `GPROXY_UPDATE_CHANNEL` | `releases` | `gproxy update` 子命令使用的 channel。它故意不同于 serve 路径变量名，以避免 `clap` env 冲突。 |
| `GPROXY_UPDATE_RESTART` | `supervisor` | `gproxy update apply` 的重启模式：`supervisor`、`re-exec` 或 `none`。 |

`GPROXY_UPDATE_PUBKEY` 是编译期变量，用于把更新验签公钥嵌入二进制；它不是运行时配置。

## 开发与迁移

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `GPROXY_INSECURE_COOKIES` | 空 | 本地明文 HTTP 开发逃生口。设为 `1` 后 admin session cookie 可以不带 `Secure`。生产 HTTPS 不应使用。 |
| `DATABASE_SECRET_KEY` | 空 | 仅用于 v1 迁移。如果旧 v1 数据库使用该 key 加密 secret，v1 reader 会先用它解密，再按 `GPROXY_MASTER_KEY` 重新密封到 v2。 |
| `RUST_LOG` | `info` | native 日志使用的标准 `tracing_subscriber` filter。 |

## Edge wrapper

wasm edge 入口不走 `clap`，由平台 wrapper 传入配置。当前部署模板会把
Turso/libSQL 数据库 URL/token 传给 wasm 持久化后端，可选传入 Upstash
cache URL/token，也可传入 `GPROXY_MASTER_KEY` 打开密封 secret。具体变量名
由平台 wrapper 决定，请以 edge 部署页面为准。

## 示例

```bash
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
GPROXY_PERSISTENCE=db \
GPROXY_DATA_DIR=/var/lib/gproxy \
GPROXY_DSN='postgres://gproxy:secret@db.internal:5432/gproxy' \
GPROXY_MASTER_KEY="$GPROXY_MASTER_KEY" \
GPROXY_ADMIN_PASSWORD="$RECOVERY_PASSWORD" \
./gproxy
```

首启导入请使用 JSON bundle：

```bash
GPROXY_IMPORT_FILE=/etc/gproxy/import.json ./gproxy
```
