---
title: 数据库后端
description: native 与 edge 持久化后端、schema 行为、secret 存储和运维取舍。
---

gproxy v2 有一个 persistence trait 和多个后端。native 部署可以使用 file
后端或 SeaORM database 后端。edge 部署在 wasm bundle 中使用面向
libSQL/Turso 的后端。

持久化层存储 control-plane 数据、authz 数据、日志、usage、rollup、
settings、tokenizer、transform rule 和 provider credential。cache 后端与
persistence 是两层。

## Native 后端

| 后端 | 选择方式 | 说明 |
| --- | --- | --- |
| SeaORM database | `GPROXY_PERSISTENCE=db` | native 默认后端。通过 SeaORM feature 支持 SQLite、PostgreSQL 和 MySQL。未设置 `GPROXY_DSN` 时，会派生 `sqlite://<absolute data_dir>/gproxy.db?mode=rwc`。 |
| File backend | `GPROXY_PERSISTENCE=file` | 在 `GPROXY_DATA_DIR` 下每张逻辑表一个 JSON 文件。只适合单实例，会对 `.gproxy.lock` 取排他 advisory lock。 |

多实例部署建议使用 native `db`。Redis cache 加 file persistence 不是安全的多节点配置；服务会警告，因为每个进程会拥有发散的文件状态。

## Edge 后端

wasm edge bundle 在启用 edge feature set 时使用 libSQL/Turso persistence。
schema 是手写 SQLite dialect DDL，与 native SeaORM entity 对齐。edge wrapper
负责把平台数据库 URL/token 传入 wasm entry point。

## DSN 示例

```text
sqlite:///var/lib/gproxy/gproxy.db?mode=rwc
postgres://gproxy:secret@127.0.0.1:5432/gproxy
mysql://gproxy:secret@127.0.0.1:3306/gproxy
```

本地开发默认 `db` 模式会在 `./data` 下创建 SQLite 文件：

```bash
./gproxy
```

显式使用 file persistence：

```bash
GPROXY_PERSISTENCE=file GPROXY_DATA_DIR=./data-file ./gproxy
```

## Schema 创建与迁移

native database 后端连接时会根据 SeaORM entity 创建表，然后运行内置
migration tracker。libSQL 后端使用匹配的 `CREATE TABLE IF NOT EXISTS`
SQL。file 后端是 schemaless JSON，但会写入 `schema_version.json` 以保持对称。

重要 schema 特性：

- provider name、route name、alias、user name 和 user-key digest 唯一；
- `routing_rules` 对 `(provider_id, operation, kind)` 唯一；
- quota 对 `(scope, scope_id)` 唯一；
- usage rollup 对 granularity、bucket 和可选维度使用复合唯一索引，使并发首次写入发生冲突并重试为累加。

当前 v2 二进制没有独立的 operator migration 命令。启动时会创建、stamp
并运行 pending schema work；失败时直接启动失败。

## 主要表组

| 分组 | 表 |
| --- | --- |
| Providers | `providers`, `credentials`, `credential_statuses`, `provider_models` |
| Routing | `routes`, `route_members`, `aliases` |
| Transform | `routing_rules`, `rule_sets`, `rules`, `provider_rule_sets` |
| Identity and authz | `orgs`, `teams`, `users`, `user_keys`, `route_permissions`, `rate_limits`, `quotas` |
| Usage and logs | `usages`, `usage_rollups`, `downstream_requests`, `upstream_requests`, `audit_logs` |
| Settings and tokenizers | `instance_settings`, `tokenizer_vocabs` |

JSON 列在 Rust record 中以 JSON-like value 表示；后端需要时以 text 存储。
金额 decimal 字段以 decimal text 存储。

## Secret 存储

`GPROXY_MASTER_KEY` 控制 v2 sealed-secret 模式。它必须是标准 base64，解码后正好 32 字节。

- 设置后，provider credential 和 user API-key ciphertext 会在存储前密封。
- 缺失时，gproxy 进入明文模式并打印警告。
- 用户密码是 Argon2 hash；恢复覆盖会先重新 hash 再存储。

export 会把 secret 解密到 JSON bundle 中，以支持 `export | import`
round-trip。请保护导出的 bundle。

`DATABASE_SECRET_KEY` 不是 v2 运行时加密密钥。它只在 legacy v1 migration
reader 读取含加密 secret 的 v1 数据库时使用。

## v1 迁移

默认 feature set 下，serve 路径可以在打开 v2 backend 前检测并迁移配置位置上的 legacy v1 SQLite 数据库。启用 feature 时也有显式 `migrate-v1` 子命令。

如果 v1 使用过加密 secret，请提供 `DATABASE_SECRET_KEY`。如果导入后的 v2
行需要按 v2 key 密封，请同时提供 `GPROXY_MASTER_KEY`。

## Retention 与日志

usage 行、请求日志、audit log 和 rollup 都在持久化层。serve 路径会启动
retention 后台任务；在 instance settings 设置 retention window 前它是 no-op。
body capture 会让表增长很快；共享服务中请谨慎开启并设置 retention。
