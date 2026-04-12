---
title: 数据库后端
description: GPROXY 支持的数据库、DSN 格式和静态加密。
---

`gproxy-storage` 通过 **SeaORM** 与 **SQLx** 同时编入三种数据库后端，
具体由 `GPROXY_DSN` (或 TOML 的 `dsn`) 决定。

## 支持的后端

| 数据库 | DSN 前缀 | 备注 |
| --- | --- | --- |
| SQLite | `sqlite:` | 默认。未设置 `GPROXY_DSN` 时，GPROXY 会在 `GPROXY_DATA_DIR` 下自动生成 SQLite 文件 DSN。 |
| PostgreSQL | `postgres:` | 通过 `sqlx-postgres` + SeaORM Postgres feature 提供。 |
| MySQL | `mysql:` | 通过 `sqlx-mysql` + SeaORM MySQL feature 提供。 |

## DSN 示例

```text
sqlite://./data/gproxy.db?mode=rwc
postgres://gproxy:secret@127.0.0.1:5432/gproxy
mysql://gproxy:secret@127.0.0.1:3306/gproxy
```

SQLite 的 `?mode=rwc` 让 SQLx 以读写方式打开，且文件不存在时创建 ——
便于首次 bootstrap。

## 连接生命周期

启动时 `SeaOrmStorage::connect()` 执行以下步骤：

1. 根据 `DATABASE_SECRET_KEY` 可选加载数据库加密器。
2. 应用每种数据库对应的连接参数 (连接池大小、超时、SQLite pragma 等)。
3. 建立连接并调用 `sync()`，把 schema 与编译期的 entity 定义对齐。

没有单独的 "运行 migrations" 命令 —— schema 同步是启动流程的一部分。
同步失败会以明确错误直接中止进程，而不是带着残缺 schema 启动。

## 静态加密

设置 `DATABASE_SECRET_KEY` 即启用数据库加密器。启用后，以下字段在写入前会用
**XChaCha20-Poly1305** 加密：

- 供应商凭证
- 用户密码 (在 Argon2 哈希之上再加密)
- 用户 API key

丢失该 key 意味着无法解密既有行，请务必在密钥管理器 / sealed 文件中备份。

:::caution
**不**支持自动密钥轮转。修改 `DATABASE_SECRET_KEY` 后，既有加密数据会解密失败 ——
需要重新创建或重新导入受影响的凭证。
:::

## 如何选择后端

- **SQLite** 适合单机部署、开发环境和小型共享实例。无需任何基础设施，
  性能足以应付绝大多数 LLM 代理场景 (热路径很少写)。
- **PostgreSQL** 适合 HA、多写或已有托管 Postgres 的环境，推荐生产默认。
- **MySQL** 同样受支持且稳定 —— 如果你的运维栈是 MySQL 就用它。
