---
title: 从 v1 迁移到 v2
description: 将 v1 SQLite 部署迁移到 GPROXY v2，并理解行为差异。
---

v2 带有一个临时 v1 SQLite 迁移器，使常见单机部署可以替换二进制后直接启动。启动时，
v2 可识别旧的 `data/gproxy.db`，读取 v1 控制面配置，创建新的 v2 数据库，并把旧库移动为备份。

这个迁移器是过渡能力，计划在 2.1 移除。仍在 v1 的实例应先经过仍包含默认
`migrate-v1` feature 的 v2 build 完成迁移，再升级到后续 v2 release。

## 自动迁移何时触发

自动迁移只在普通 serve path 上触发，并且必须全部满足：

- 二进制包含默认 `migrate-v1` feature；
- 当前没有运行 `import`、`export`、`update` 或 `migrate-v1` 子命令；
- persistence 是 `db`；
- 目标 DSN 是真实 SQLite 文件；
- 文件存在并看起来是 v1 schema：有 `global_settings` 和 `providers`，且没有 v2
  `orgs`、`routes`、`schema_migrations`。

native v2 默认值匹配常见 v1 路径：`GPROXY_PERSISTENCE=db`，未设置 `GPROXY_DSN` 时使用
`<data-dir>/gproxy.db`。

这些情况不会自动迁移：

| 情况 | 结果 |
| --- | --- |
| 新安装，没有 `gproxy.db` | 创建普通 v2 数据库。 |
| 已经是 v2 数据库 | 跳过迁移。 |
| `--persistence=file` | 不执行 v1 SQLite 迁移。 |
| PostgreSQL 或 MySQL 目标 | 使用显式 `migrate-v1 --to <dsn>`。 |
| `sqlite::memory:` | 没有可接管文件。 |

## Drop-In 升级

启动 v2 前先停掉 v1。v1 使用 SQLite WAL 时，迁移器可以处理 sidecar 文件，但旧进程必须停止写入。

```bash
systemctl stop gproxy
cp data/gproxy.db data/gproxy.db.manualbak
install -m 0755 gproxy-v2 /usr/local/bin/gproxy

GPROXY_DATA_DIR=./data \
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
gproxy
```

成功后：

- `data/gproxy.db` 是新的 v2 数据库；
- 旧 v1 数据库被移动到 `data/gproxy.db.v1.bak`；
- 如果备份名已存在，v2 会使用下一个可用后缀，例如 `.v1.bak.2`。

这个过程幂等。live database 已经是 v2 schema 后，后续启动会跳过迁移。

## 加密的 v1 数据

v1 使用 `DATABASE_SECRET_KEY` 加密 `credentials.secret_json` 或
`user_keys.api_key_ciphertext`。迁移时提供同一个值：

```bash
DATABASE_SECRET_KEY='<v1 database secret>' \
GPROXY_MASTER_KEY='<base64-encoded 32-byte v2 master key, optional>' \
gproxy --data-dir ./data
```

迁移器用 `DATABASE_SECRET_KEY` 打开 v1 secret，把明文控制面数据映射成 v2 import bundle，
再通过 v2 import path 写入。设置 `GPROXY_MASTER_KEY` 时，导入 secret 按 v2 规则 sealed；
不设置时，v2 以明文 secret 模式运行并 warning。

如果存在加密 v1 数据但缺少正确 `DATABASE_SECRET_KEY`，迁移失败，原数据库不会被替换。

## 离线迁移

需要 dry run、PostgreSQL/MySQL 目标或可控维护窗口时，使用显式子命令：

```bash
gproxy migrate-v1 --from ./data/gproxy.db --dry-run

gproxy --data-dir ./data migrate-v1 --from ./data/gproxy.db

gproxy migrate-v1 \
  --from ./old/gproxy.db \
  --to 'postgres://gproxy:secret@db.internal:5432/gproxy'
```

离线迁移只读取 `--from` 指向的 v1 SQLite 文件，并把映射后的 v2 记录写入目标 DSN。
它不会交换文件，也不会创建 `.v1.bak`。目标应为空 v2 数据库，避免 id 冲突。

## 迁移内容

v2 迁移控制面配置，不迁移运行时历史。

| v1 数据 | v2 结果 |
| --- | --- |
| `users` | `users`，挂到合成 org `default`。 |
| `user_keys` | `user_keys`，解密后通过 v2 import 重写。 |
| `providers` | `providers`，保留 id、name、channel、label、settings。 |
| `credentials` | `credentials`，解密并重新 sealed，默认 weight 100。 |
| `models` | `provider_models`，保留 provider model 元数据。 |
| 多个 provider 上同名 `model_id` | 一个 v2 route，多个 members。 |
| `user_quotas` | user-scoped quotas。 |
| `user_model_permissions` | route permissions，继承 v1 glob。 |
| `user_rate_limits` | rate limits，继承 v1 route pattern。 |
| `global_settings` | instance settings，包括 proxy、logging、usage、update channel。 |

不迁移：

- usage billing history；
- upstream/downstream request logs；
- file records；
- credential health state；
- v1 `routing_json` 的逐条自定义规则。

## 行为差异

| 差异 | 迁移后检查 |
| --- | --- |
| v1 model 变成 v2 route | 聚合模式下 client 调用 route name。 |
| 同名模型跨 provider | v2 保留多个 route members，而不是把其中一个当覆盖。 |
| 默认 route strategy | 合成 route 使用 `failover`，weight 100，tier 0。 |
| v1 `routing_json` | 不翻译；v2 seed channel defaults，未知 channel 会 warning。 |
| pricing | tiered/flex/scale/priority pricing 收敛为 v2 flat pricing 字段。 |
| TLS spoof | v1 `spoof_emulation` 变成 v2 instance-setting boolean。 |

迁移后在 Console 中复核 provider channels、route members、routing rules 和 pricing。

## 回滚

停掉 v2，删除 v2 数据库文件，还原 v1 备份：

```bash
systemctl stop gproxy
rm -f data/gproxy.db data/gproxy.db-wal data/gproxy.db-shm
mv data/gproxy.db.v1.bak data/gproxy.db

[ -f data/gproxy.db.v1.bak-wal ] && mv data/gproxy.db.v1.bak-wal data/gproxy.db-wal
[ -f data/gproxy.db.v1.bak-shm ] && mv data/gproxy.db.v1.bak-shm data/gproxy.db-shm

systemctl start gproxy
```

如果迁移器创建的是 `.v1.bak.2` 或其它后缀，按实际备份文件名恢复。
