---
title: 环境变量
description: gproxy 启动时读取的全部环境变量。
---

gproxy 的启动参数定义在 `apps/gproxy/src/main.rs`，由
[`clap`](https://crates.io/crates/clap) 解析。每个 CLI flag 都有对应的环境
变量；**CLI 参数优先于环境变量**。

## 运行时

| 变量 | 默认 | 必填 | 说明 |
| --- | --- | --- | --- |
| `GPROXY_HOST` | `127.0.0.1` | 否 | 监听地址。 |
| `GPROXY_PORT` | `8787` | 否 | 监听端口。 |
| `GPROXY_PROXY` | 无 | 否 | 调用上游 LLM 时使用的 HTTP 代理。 |
| `GPROXY_SPOOF` | `chrome_136` | 否 | TLS 指纹伪装名。 |

## 管理员 bootstrap

仅在首次启动 bootstrap 管理员账号时使用。

| 变量 | 默认 | 必填 | 说明 |
| --- | --- | --- | --- |
| `GPROXY_ADMIN_USER` | `admin` | 否 | 管理员用户名。 |
| `GPROXY_ADMIN_PASSWORD` | 无 | 否 | 管理员密码。若未设置且需要创建管理员，gproxy **自动生成并打印一次**。 |
| `GPROXY_ADMIN_API_KEY` | 无 | 否 | 管理员 API key。若未设置且需要创建管理员，gproxy **自动生成并打印一次**。 |

## 存储

| 变量 | 默认 | 必填 | 说明 |
| --- | --- | --- | --- |
| `GPROXY_DSN` | `sqlite://<data_dir>/gproxy.db?mode=rwc` (自动生成) | 否 | 数据库 DSN。 |
| `GPROXY_DATA_DIR` | `./data` | 否 | 数据目录，默认 SQLite 文件和运行态数据基于此目录。 |
| `GPROXY_CONFIG` | `gproxy.toml` | 否 | 首次初始化使用的种子 TOML 路径。 |
| `DATABASE_SECRET_KEY` | 无 | 否 | 静态加密密钥。设置后密码、API key、凭证会用 XChaCha20-Poly1305 加密。 |
| `GPROXY_REDIS_URL` | 无 | 否 | Redis DSN，仅当二进制启用了 `redis` feature 时生效。 |

## 行为说明

- **CLI 优先。** 以上变量都有同名 CLI flag。同时设置时，CLI 获胜。
- **数据库中的全局设置优先。** 若数据库已经存在 `global_settings` 行，且启动时
  你**没有**显式传 `GPROXY_DSN` / `GPROXY_DATA_DIR`，进程会按**数据库中持久化**的
  配置重新连接 —— 不是环境变量中的默认值。这是有意为之：方便你通过控制台
  永久性地修改 host / port / DSN。
- **bootstrap 日志。** 自动生成的管理员密码和 API key **仅在首次创建时打印一次**
  (INFO 级别)。请务必捕获首次启动日志，或直接通过环境变量显式传入。

## 示例

最小 dev 启动：

```bash
./gproxy
```

类生产启动：

```bash
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
GPROXY_DATA_DIR=/var/lib/gproxy \
GPROXY_DSN=postgres://gproxy:secret@db.internal:5432/gproxy \
DATABASE_SECRET_KEY=$(cat /run/secrets/gproxy_db_key) \
GPROXY_CONFIG=/etc/gproxy/seed.toml \
GPROXY_ADMIN_USER=ops \
GPROXY_ADMIN_PASSWORD=$(cat /run/secrets/gproxy_admin_pw) \
./gproxy
```
