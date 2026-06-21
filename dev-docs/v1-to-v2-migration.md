# gproxy v1 到 v2 迁移

v2 带有一个临时的 v1 SQLite 迁移器，目标是让常见单机部署可以
**替换二进制后直接启动**：v2 在启动时识别旧的 `data/gproxy.db`，只读导出
v1 控制面配置，生成新的 v2 数据库，再把旧库备份到旁边。

这个能力是过渡功能，计划在 **2.1** 移除。仍在 v1 上运行的实例应先升级到
带 `migrate-v1` feature 的 v2 版本并完成迁移，再进入后续 v2 版本。

## 支持范围

自动迁移只在服务启动路径上触发，并且必须满足这些条件：

- 二进制包含默认 feature 集中的 `migrate-v1`。
- 没有执行 `import`、`export`、`update`、`migrate-v1` 等子命令。
- 持久化后端是 `db`，目标 DSN 是一个真实的 SQLite 文件。
- SQLite 文件存在，且看起来是 v1 schema：包含 `global_settings` 和
  `providers`，但没有 v2 的 `orgs`、`routes` 或 `schema_migrations`。

默认 v2 配置正好满足这个路径：`GPROXY_PERSISTENCE=db`，未设置
`GPROXY_DSN` 时会使用 `<data_dir>/gproxy.db`。这也是 v1 默认数据库路径。

不会自动迁移的情况：

| 情况 | 结果 |
| --- | --- |
| 新安装，没有 `gproxy.db` | 正常创建 v2 数据库。 |
| 目标已经是 v2 数据库 | 跳过迁移。 |
| `--persistence=file` | 不触发迁移；file 后端不是 v1 SQLite 兼容目标。 |
| PostgreSQL / MySQL DSN | 不触发启动迁移；需要显式 `migrate-v1 --to <dsn>`。 |
| `sqlite::memory:` | 不触发迁移；没有可接管的文件。 |

## 推荐升级路径

停掉 v1 后再启动 v2。v1 使用 WAL 时，迁移器会读取 `-wal` 内容并在换库时一并移动
sidecar 文件，但前提是旧进程已经停止，不再写入数据库。

```bash
# 1. 停掉 v1 进程
systemctl stop gproxy

# 2. 额外做一份人工备份，便于脱离迁移器回退
cp data/gproxy.db data/gproxy.db.manualbak

# 3. 换成 v2 二进制
install -m 0755 gproxy-v2 /usr/local/bin/gproxy

# 4. 用原 data 目录启动 v2
GPROXY_DATA_DIR=./data \
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
gproxy
```

如果你之前用 CLI 参数启动，也可以继续使用：

```bash
gproxy --data-dir ./data --host 0.0.0.0 --port 8787
```

迁移成功后，`data/gproxy.db` 是新的 v2 数据库，旧的 v1 数据库被移动为
`data/gproxy.db.v1.bak`。如果这个备份名已经存在，迁移器会使用
`gproxy.db.v1.bak.2`、`gproxy.db.v1.bak.3` 这样的下一个可用文件名。

启动日志会包含类似信息：

```text
WARN v1 database detected - migrating to v2 in place (original backed up)
INFO v1 -> v2 migration complete report=Report { providers: 6, credentials: 6, models: 400, ... }
WARN v1 database backed up; v2 database is now live at .../data/gproxy.db
INFO gproxy v2 listening on http://0.0.0.0:8787
```

迁移是幂等的。第一次成功后，`gproxy.db` 已经是 v2 schema，后续启动会直接跳过。
如果进程在换库窗口中断，下一次启动会尝试完成未完成的临时文件交换，或者把备份恢复为
源库后重新迁移。

## 加密配置

v1 的加密密钥名是 `DATABASE_SECRET_KEY`。如果 v1 曾经用它加密
`credentials.secret_json` 或 `user_keys.api_key_ciphertext`，迁移时必须继续提供
同一个值：

```bash
DATABASE_SECRET_KEY='<v1 database secret>' \
GPROXY_MASTER_KEY='<base64-encoded 32-byte v2 master key, optional>' \
gproxy --data-dir ./data
```

迁移器会：

1. 用 `DATABASE_SECRET_KEY` 解开 v1 secret。
2. 把明文配置导入 v2 bundle。
3. 通过 v2 的 import 路径重新写入目标库。

`GPROXY_MASTER_KEY` 是 v2 的密封密钥，必须是标准 base64 编码后的 32 字节值。
设置后，导入的凭证和 API key 会按 v2 规则重新封装；不设置时，v2 会以明文模式
运行并在启动日志中警告。

如果 v1 数据库里存在 `enc:v2:` 或 JSON 加密 envelope，但迁移时没有提供正确的
`DATABASE_SECRET_KEY`，迁移会失败，不会替换原库。

## 显式离线迁移

需要迁到 PostgreSQL/MySQL，或者想先查看迁移规模时，使用 `migrate-v1` 子命令：

```bash
# 只读扫描 v1 数据库，不写目标库
gproxy migrate-v1 --from ./data/gproxy.db --dry-run

# 导入当前默认 db 目标；默认目标来自 --persistence/--data-dir/--dsn
gproxy --data-dir ./data migrate-v1 --from ./data/gproxy.db

# 显式导入到 PostgreSQL
gproxy migrate-v1 \
  --from ./old/gproxy.db \
  --to 'postgres://gproxy:secret@db.internal:5432/gproxy'
```

离线迁移不会替换文件，也不会创建 `.v1.bak`。它只读取 `--from` 指向的 v1
SQLite 文件，并把映射后的 v2 bundle 写入目标 DSN。目标库应当是空的 v2
控制面库，避免与已有 provider、route、user id 冲突。

## 迁移内容

v2 只迁移控制面配置，不迁移运行时历史数据。

| v1 数据 | v2 结果 |
| --- | --- |
| `users` | `users`，所有用户挂到合成组织 `default`。 |
| `user_keys` | `user_keys`，API key 解密后由 v2 import 路径重新写入。 |
| `providers` | `providers`，保留 id、name、channel、label、settings。 |
| `credentials` | `credentials`，secret 解密后重新封装，默认 weight 为 100。 |
| `models` | `provider_models`，保留 provider、model id、display name、enabled。 |
| 同名 `model_id` | 合成同一个 v2 route，多个 provider model 变成多个 route member。 |
| `user_quotas` | `quotas`，scope 为 user。 |
| `user_model_permissions` | `route_permissions`，v1 model glob 按 v2 route glob 继承。 |
| `user_rate_limits` | `rate_limits`，route pattern 继承 v1 model pattern。 |
| `global_settings` | `instance_settings`，迁移 proxy、日志、usage、update channel 等运行偏好。 |

不迁移：

- usage 计费历史；
- upstream/downstream 请求日志；
- 文件记录；
- 凭证健康状态；
- v1 `routing_json` 的逐条自定义规则。

这些数据不会影响控制面启动，但迁移后会从 v2 的新表重新开始记录。

## 行为差异

v2 的路由模型和 v1 不完全相同，迁移器会尽量保留可调用性，但不会假装旧语义完全相等。

| 差异 | 说明 |
| --- | --- |
| 路由由 model 变成 route | 每个唯一 `model_id` 合成一个 v2 route，route 名等于 model id。 |
| 同名模型跨 provider | v1 倾向于后加载者覆盖；v2 会保留所有成员，并按 route 策略执行。 |
| route 默认策略 | 合成 route 使用 `failover`，成员初始 weight 为 100、tier 为 0。 |
| provider routing | 不翻译 v1 `routing_json`，而是按 v2 channel 默认规则重新 seed。 |
| 未知 channel | provider 和 credential 会迁移，但默认 routing seed 会警告；需要在 console 中改成 v2 支持的 channel。 |
| 计价模型 | v1 tiered/flex/scale/priority 价格会收敛成 v2 扁平的 `input`、`output`、`cache_read`、`cache_creation`。 |
| TLS spoof | v1 的具体 `spoof_emulation` 字符串在 instance settings 中收敛成 v2 布尔开关。 |

迁移后建议重点复核 provider channel、route 成员、routing rules 和价格。

## 验证清单

启动 v2 后，至少检查这些项：

```bash
gproxy --data-dir ./data --host 127.0.0.1 --port 8787
```

- Console 能登录，admin 用户存在。
- providers、credentials、models、routes、users、user keys 数量符合预期。
- 每个迁移过来的 provider 有默认 routing rules；未知 channel 已修正。
- 重要模型的 pricing 在 Console 中符合预期。
- 用迁移过来的 user key 调一次聚合入口：

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H 'Authorization: Bearer <migrated-user-key>' \
  -H 'Content-Type: application/json' \
  -d '{"model":"<route-name>","messages":[{"role":"user","content":"ping"}]}'
```

## 回滚

迁移不会修改 v1 源库内容；自动路径只是把它移动到备份文件。要回滚：

```bash
systemctl stop gproxy
rm -f data/gproxy.db data/gproxy.db-wal data/gproxy.db-shm
mv data/gproxy.db.v1.bak data/gproxy.db

# 如果存在 sidecar 备份，也一起还原
[ -f data/gproxy.db.v1.bak-wal ] && mv data/gproxy.db.v1.bak-wal data/gproxy.db-wal
[ -f data/gproxy.db.v1.bak-shm ] && mv data/gproxy.db.v1.bak-shm data/gproxy.db-shm

# 换回 v1 二进制后启动
systemctl start gproxy
```

如果迁移器使用了 `gproxy.db.v1.bak.2` 这样的备份名，按实际文件名还原即可。

## 升级窗口

这套迁移代码明确标注为 `MIGRATE-V1 (remove in 2.1)`。不要把 v1 数据库直接带到
2.1 或更高版本；先用包含迁移器的 v2 版本完成一次迁移，再升级。

## English

# Migrating From gproxy v1 To v2

v2 includes a temporary v1 SQLite migrator so common single-node deployments can
**replace the binary and start directly**. On startup, v2 detects an old
`data/gproxy.db`, exports the v1 control-plane configuration in read-only mode,
creates a new v2 database, and moves the old database next to it as a backup.

This is a transition feature and is planned for removal in **2.1**. Instances
still running v1 should first upgrade to a v2 build that includes the
`migrate-v1` feature, complete the migration, and only then move to later v2
versions.

## Supported Scope

Automatic migration runs only on the service startup path and only when all of
these conditions are true:

- the binary includes the default `migrate-v1` feature;
- no `import`, `export`, `update`, `migrate-v1`, or similar subcommand is being
  executed;
- persistence is `db` and the target DSN is a real SQLite file;
- the SQLite file exists and looks like v1 schema: it has `global_settings` and
  `providers`, but not v2 `orgs`, `routes`, or `schema_migrations`.

The default v2 configuration matches this path: `GPROXY_PERSISTENCE=db`, and
without `GPROXY_DSN` it uses `<data_dir>/gproxy.db`, which is also the v1 default
database path.

Cases that do not auto-migrate:

| Case | Result |
| --- | --- |
| Fresh install without `gproxy.db` | Creates a normal v2 database. |
| Target is already a v2 database | Skips migration. |
| `--persistence=file` | Does not migrate; file backend is not a v1 SQLite-compatible target. |
| PostgreSQL / MySQL DSN | Does not run startup migration; use explicit `migrate-v1 --to <dsn>`. |
| `sqlite::memory:` | Does not migrate; there is no file to take over. |

## Recommended Upgrade Path

Stop v1 before starting v2. If v1 uses WAL, the migrator can read `-wal` content
and move sidecar files during the database swap, but only if the old process has
stopped writing.

```bash
# 1. Stop the v1 process
systemctl stop gproxy

# 2. Make an extra manual backup for rollback outside the migrator
cp data/gproxy.db data/gproxy.db.manualbak

# 3. Replace the binary with v2
install -m 0755 gproxy-v2 /usr/local/bin/gproxy

# 4. Start v2 with the same data directory
GPROXY_DATA_DIR=./data \
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
gproxy
```

If you previously used CLI flags, you can keep using them:

```bash
gproxy --data-dir ./data --host 0.0.0.0 --port 8787
```

After success, `data/gproxy.db` is the new v2 database and the old v1 database is
moved to `data/gproxy.db.v1.bak`. If that name already exists, the migrator uses
the next available name such as `gproxy.db.v1.bak.2` or `gproxy.db.v1.bak.3`.

Startup logs should include messages like:

```text
WARN v1 database detected - migrating to v2 in place (original backed up)
INFO v1 -> v2 migration complete report=Report { providers: 6, credentials: 6, models: 400, ... }
WARN v1 database backed up; v2 database is now live at .../data/gproxy.db
INFO gproxy v2 listening on http://0.0.0.0:8787
```

The migration is idempotent. After the first successful run, `gproxy.db` is v2
schema and later starts skip migration. If the process is interrupted during the
swap window, the next start attempts to complete the temporary file swap or
restore the backup as the source and migrate again.

## Encryption

v1 used `DATABASE_SECRET_KEY` as the encryption key name. If v1 encrypted
`credentials.secret_json` or `user_keys.api_key_ciphertext`, provide the same
value during migration:

```bash
DATABASE_SECRET_KEY='<v1 database secret>' \
GPROXY_MASTER_KEY='<base64-encoded 32-byte v2 master key, optional>' \
gproxy --data-dir ./data
```

The migrator:

1. opens v1 secrets with `DATABASE_SECRET_KEY`;
2. imports the plaintext configuration into a v2 bundle;
3. writes it through the v2 import path.

`GPROXY_MASTER_KEY` is the v2 sealing key and must be a standard base64-encoded
32-byte value. When set, imported credentials and API keys are sealed using v2
rules. When absent, v2 runs in plaintext mode and logs a warning.

If the v1 database contains `enc:v2:` or JSON encryption envelopes and the
correct `DATABASE_SECRET_KEY` is missing, migration fails and the original
database is not replaced.

## Explicit Offline Migration

Use the `migrate-v1` subcommand when migrating to PostgreSQL/MySQL or when you
want to inspect migration size first:

```bash
# Read-only scan of the v1 database; does not write target
gproxy migrate-v1 --from ./data/gproxy.db --dry-run

# Import into the current default db target from --persistence/--data-dir/--dsn
gproxy --data-dir ./data migrate-v1 --from ./data/gproxy.db

# Explicit import to PostgreSQL
gproxy migrate-v1 \
  --from ./old/gproxy.db \
  --to 'postgres://gproxy:secret@db.internal:5432/gproxy'
```

Offline migration does not replace files or create `.v1.bak`. It reads only the
v1 SQLite file passed by `--from` and writes the mapped v2 bundle into the target
DSN. The target should be an empty v2 control-plane database to avoid provider,
route, or user id collisions.

## Migrated Data

v2 migrates control-plane configuration only, not runtime history.

| v1 data | v2 result |
| --- | --- |
| `users` | `users`, all attached to a synthetic `default` organization. |
| `user_keys` | `user_keys`, decrypted and rewritten by the v2 import path. |
| `providers` | `providers`, preserving id, name, channel, label, settings. |
| `credentials` | `credentials`, decrypted and re-sealed, default weight 100. |
| `models` | `provider_models`, preserving provider, model id, display name, enabled. |
| Same `model_id` | One synthetic v2 route with multiple route members. |
| `user_quotas` | `quotas`, user scope. |
| `user_model_permissions` | `route_permissions`, preserving v1 model globs as v2 route globs. |
| `user_rate_limits` | `rate_limits`, preserving v1 model pattern as route pattern. |
| `global_settings` | `instance_settings`, including proxy, logging, usage, update-channel preferences. |

Not migrated:

- usage billing history;
- upstream/downstream request logs;
- file records;
- credential health state;
- individual custom rules from v1 `routing_json`.

These do not affect control-plane startup. v2 begins recording them into new
tables after migration.

## Behavior Differences

v2's routing model is not identical to v1. The migrator preserves callability as
much as possible, but it does not pretend old semantics are exactly the same.

| Difference | Explanation |
| --- | --- |
| Model becomes route | Each unique `model_id` becomes one v2 route with the same name. |
| Same model across providers | v1 tended to let later entries override; v2 keeps all members and applies the route strategy. |
| Default route strategy | Synthetic routes use `failover`; members start with weight 100 and tier 0. |
| Provider routing | v1 `routing_json` is not translated; v2 seeds channel defaults instead. |
| Unknown channel | Provider and credential migrate, but default routing seed warns; fix the channel in Console. |
| Pricing model | v1 tiered/flex/scale/priority pricing collapses into v2 flat `input`, `output`, `cache_read`, `cache_creation`. |
| TLS spoof | The exact v1 `spoof_emulation` string becomes a v2 instance-setting boolean. |

After migration, review provider channel, route members, routing rules, and
pricing carefully.

## Verification Checklist

Start v2 and check at least:

```bash
gproxy --data-dir ./data --host 127.0.0.1 --port 8787
```

- Console login works and the admin user exists.
- provider, credential, model, route, user, and user-key counts look right.
- each migrated provider has default routing rules; unknown channels are fixed.
- important model pricing is correct in Console.
- one migrated user key can call the aggregated entry:

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H 'Authorization: Bearer <migrated-user-key>' \
  -H 'Content-Type: application/json' \
  -d '{"model":"<route-name>","messages":[{"role":"user","content":"ping"}]}'
```

## Rollback

Migration does not modify the v1 source content; the automatic path only moves
it to a backup file. To roll back:

```bash
systemctl stop gproxy
rm -f data/gproxy.db data/gproxy.db-wal data/gproxy.db-shm
mv data/gproxy.db.v1.bak data/gproxy.db

# Restore sidecar backups too if present
[ -f data/gproxy.db.v1.bak-wal ] && mv data/gproxy.db.v1.bak-wal data/gproxy.db-wal
[ -f data/gproxy.db.v1.bak-shm ] && mv data/gproxy.db.v1.bak-shm data/gproxy.db-shm

# Switch back to the v1 binary, then start
systemctl start gproxy
```

If the migrator used a backup name such as `gproxy.db.v1.bak.2`, restore that
actual file instead.

## Upgrade Window

The migration code is explicitly marked `MIGRATE-V1 (remove in 2.1)`. Do not
carry a v1 database directly into 2.1 or later. First complete migration with a
v2 version that still includes the migrator, then upgrade.
