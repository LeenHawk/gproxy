---
title: "v2 到 v3 迁移"
description: "把 v2 部署迁移到 v3：操作员需要了解的变化，以及 gproxy migrate 如何把 v2 SQLite 数据库导入全新的 v3 store"
---

v3 是一次带全新 store 的重写。v3 二进制不会原地打开 v2 数据库；取而代之，
`gproxy migrate --from-v2` 以只读方式读取 v2 SQLite 文件，并把内容写入一个 v3
store。v2 本身继续在 `main` 分支上以 `v2.x.y` tag 维护。本页取代 v2 文档中的
「从 v1 迁移到 v2」。

## 操作员视角的变化

| 方面 | v2 | v3 |
| --- | --- | --- |
| 配置 | 命令行参数与环境变量 | 命令行参数、环境变量、`./.env`、`<data-dir>/.env`；没有配置文件格式。含义相同的名称得以保留（`GPROXY_HOST`、`GPROXY_PORT`、`GPROXY_DATA_DIR`、`GPROXY_DSN`、`GPROXY_REDIS_URL`、`GPROXY_MASTER_KEY`、`GPROXY_ADMIN_USER`、`GPROXY_ADMIN_PASSWORD`、`UPSTASH_URL`、`UPSTASH_TOKEN`） |
| 持久化 | `GPROXY_PERSISTENCE=db` | `sqlite`（默认）、`libsql`、`postgres` 或 `mysql`；`db` 会被拒绝 |
| Edge 数据库 | `TURSO_URL`、`TURSO_TOKEN` | `GPROXY_LIBSQL_URL`、`GPROXY_LIBSQL_AUTH_TOKEN` |
| 首次启动导入 | `GPROXY_IMPORT_FILE` | 已移除；改用 `gproxy migrate` 或控制台的配置导入 |
| Web 界面 | `/console` | `/admin` 控制台、`/portal` 用户门户、`/` 公开站点；API 位于 `/admin/api/**` 与 `/portal/api/**` |
| 容器 | `latest`、`-musl`、多架构；数据在 `/app/data` | 仅 `ghcr.io/leenhawk/gproxy:<tag>`，linux/amd64；数据在 `/var/lib/gproxy`；以 `gproxy` 用户运行 |
| 更新 channel | 实例设置 `update_channel` | `releases`、`staging`、`dev`；alpha 构建位于 `dev`。该设置会被导入 |
| 规则 | 重写规则与消息重写 | 统一的规则工作区：附加到 Provider 的规则集，以及每个 Provider 的路由规则 |

见[配置](/zh-cn/reference/configuration/)、[容器部署](/zh-cn/deployment/docker/)
和[路由规则与规则集](/zh-cn/guides/rules/)。

## migrate 子命令

```sh
gproxy migrate --from-v2 <path> [--from-v2-master-key <base64>] [--apply] [--merge]
```

| 参数 | 含义 |
| --- | --- |
| `--from-v2 <path>` | v2 SQLite 数据库路径。以只读方式打开，绝不修改。 |
| `--from-v2-master-key <base64>` | v2 的 master key（标准 base64，32 字节），仅在 v2 加密保存了密钥材料时需要。 |
| `--apply` | 执行写入。不带它时命令是一次 dry run。 |
| `--merge` | 允许导入到已经包含数据的 v3 store。 |

目标 store 由正常服务时的配置决定：与 `gproxy` 启动时相同的命令行参数、环境变量和
`.env` 文件（`GPROXY_DATA_DIR`、`GPROXY_PERSISTENCE`、`GPROXY_DSN`、
`GPROXY_LIBSQL_*`、`GPROXY_MASTER_KEY`）。目标可以是数据目录中的 SQLite，也可以是
任何其他后端。

dry run 会读取并解密源数据、校验、打印报告，不写任何内容，也完全不打开目标。带
`--apply` 时，命令打开目标，检查该源是否已导入过，检查目标为空（或已给出
`--merge`），重新加密每个密钥材料，写入全部行，并记录一个标记。当报告列出问题或
没有任何内容被写入时，进程以非零状态退出，并输出
`v2 migration was not applied; resolve the reported rows first`。

## 报告

```text
v2 migration: dry run
  organizations: 1 importable (1 found)
  users: 3 importable (3 found)
  user_keys: 5 importable (5 found)
  providers: 4 importable (4 found)
  credentials: 6 importable (6 found)
  routes: 8 importable (8 found)
  route_members: 11 importable (11 found)
  ...
  usage: 1520 importable (1520 found)
dry run wrote nothing; rerun with --apply to import
```

每行给出实体名、源中有多少行、以及多少行可以导入。`--apply` 之后各行变为
`N imported (M found)`。相关时还会出现两个小节：`existing target rows:` 列出目标
已有的内容，`unrecoverable rows:` 以 `entity id=N: reason` 列出每一行无法导入的
原因。已导入过的源会输出
`this v2 source was already imported; no rows were written`。

## 导入内容

| v2 数据 | v3 结果 |
| --- | --- |
| `orgs`、`teams`、`users` | 组织、团队、用户。密码哈希原样保留，因此管理员和门户用户的密码不变。每个 `is_admin` 用户还会获得一条允许全部的权限。 |
| `user_keys` | API 密钥。恢复出明文密钥，计算其 v3 摘要（去掉 `sk-`/`at-` 前缀后的 SHA-256，摘要版本 1）和 12 字符前缀，再重新加密保存。客户端继续使用相同的密钥。 |
| `providers` | 名称、设置、策略、代理和指纹相同的 Provider。旧的 channel id 会被规范化：`kimiapi` 和 `kimicode` 变为 `kimi`；`opencodezen` 和 `opencodego` 变为 `opencode`，并把 `tier` 设为 `zen` 或 `go`。 |
| `credentials` | 凭证，解密后重新加密；权重、RPM/TPM 限制、代理、指纹和启用标志保留。 |
| `routes`、`route_members` | 路由（最大尝试次数 6）及带层级、权重和上游模型的成员。每条路由对应一个以路由名命名的对外模型。 |
| `provider_models` | Provider 模型元数据：显示名、变体、上下文窗口、最大输出、思考标志。 |
| `aliases` | 别名；Provider 为 `*` 时成为全局别名，`sort_order` 变为优先级。 |
| `quotas` | 组织、团队或用户范围的配额，六个窗口全部保留。 |
| `price_rules` | 价格规则。`exact` 保留模型名；`contains` 变为 glob `*text*`。`pricing_tiers_json` 变为 `tiers`。精确规则排在最前，然后是更长的 `contains` 模式。 |
| `price_rule_rates` | 维度化价格费率。`cache_read_tokens` 改名为 `cached_input_tokens`。没有显式费率的规则会从 v2 的平铺字段合成七条按 1,000,000 token 计价的费率：输入、输出、缓存读取、缓存创建 5m/30m/1h、图像输出。 |
| `routing_rules`、`rule_sets`、`rules`、`provider_rule_sets` | 相同的实体，id 重新映射。 |
| `instance_settings`（第一行） | 实例名、代理、用量开关、分词器下载、上传并发、更新 channel 与自动检查、保留天数、数据库大小上限、四个日志开关、脱敏覆盖。`inherit_system_proxy` 设为 `false`。 |
| `usages` | 用量行，并在导入时重新计算小时汇总。图像输出与缓存创建计数移入 `metrics`；`route_name`、`kind`、`thread_id` 作为维度 `v2_route`、`v2_kind`、`v2_thread_id` 保留。 |

## 不导入的内容

导入器只读取上述表。以下 v2 表会被跳过：

| v2 表 | 后果 |
| --- | --- |
| `route_permissions` | 没有任何权限行被带过来。当没有权限匹配调用方时 v3 会拒绝请求。管理员之所以仍有访问权，仅仅是因为导入器为他们显式写入了一条允许全部的权限；其他所有用户、团队或组织都需要在客户端恢复使用前授予权限。 |
| `rate_limits` | 在控制台中重新创建限流。 |
| `upstream_requests`、`downstream_requests`、`audit_logs` | 请求日志和管理审计从零开始。 |
| `credential_statuses`、`credential_model_statuses`、`credential_quota_cycles`、`credential_quota_cycle_models`、`credential_usage_daily` | 凭证健康与配额周期状态由实际流量重建。 |
| `usage_rollups` | 由导入的用量行重新计算。 |
| `tokenizer_vocabs`、`codex_task_bindings`、`gproxy_kv` | 重新下载词表；缓存和任务绑定是临时数据。 |

## 密钥材料

v2 加密保存的值是带 `kek_id`、`wrapped_dek`、`nonce` 和 `ciphertext` 的信封。
导入器绝不复制密文：每个凭证密钥和 API 密钥都在内存中解密，再用
`GPROXY_MASTER_KEY` 提供的 v3 密钥（AES-256-GCM）重新加密，或在 v3 未配置密钥时
以明文保存。加密的 v2 值需要 `--from-v2-master-key`；明文的 v2 值什么都不需要。
无法打开的密钥材料会列在 `unrecoverable rows` 下并阻断整个导入，因此要么提供正确的
密钥，要么先在 v2 中删除那一行。

## 校验与阻断

写入之前，源数据必须自洽：团队引用一个组织，密钥引用一个用户，凭证和路由规则引用
一个 Provider，路由成员引用一条路由和一个 Provider，别名引用一个 Provider 名称，
配额引用存在的主体，费率引用一条价格规则，规则引用一个规则集。权重、限制、计数器和
单位大小必须非负。匹配类型既非 `exact` 也非 `contains`、或模式中含 `*` 的价格规则
无法表示为 v3 的 glob。只接受一行 `instance_settings`，且名称不能为空。一行失败会
连带移除引用它的行，每次移除都会列出。任何列出的问题都意味着不会写入任何内容。

对目标有两条规则：

- **非空目标需要 `--merge`。** 否则报告以
  `target store: is not empty; rerun with --merge to combine stores` 结束，并列出
  已有行数。带 `--merge` 时，导入的行获得新的 id，与已有行并存。
- **重复导入是幂等的。** 成功导入后会记录设置项
  `v2_import_<源路径解析后的 sha256>`。对同一文件再次运行命令时，报告它已被导入并
  以零状态退出。标记以路径为键，因此请始终从同一位置导入同一文件。

## 操作步骤

1. 停止 v2，让数据库处于静止状态：
   `systemctl stop gproxy` 或 `docker stop gproxy`。
2. 复制数据库，若存在 `-wal` 和 `-shm` 附属文件也一并复制：

   ```sh
   mkdir -p /srv/v2 && cp data/gproxy.db* /srv/v2/
   ```

3. 决定 v3 的目标与密钥。把设置放在二进制服务时会读取的位置，例如
   `/var/lib/gproxy/.env`：

   ```sh
   GPROXY_HOST=0.0.0.0
   GPROXY_MASTER_KEY=<standard base64, 32 bytes, optional>
   ```

4. dry run 并阅读报告：

   ```sh
   gproxy --data-dir /var/lib/gproxy migrate --from-v2 /srv/v2/gproxy.db \
     --from-v2-master-key "$V2_MASTER_KEY"
   ```

5. 用相同参数加上 `--apply` 执行。
6. 用同一数据目录启动 v3 并打开 `/admin`。使用 v2 的管理员凭据登录；由于管理员
   已被导入，不会出现初始化表单。
7. 验证：Provider 页显示预期的通道和凭证数量，路由页显示成员和对外模型，价格页
   显示规则和费率，用量页显示历史。授予权限后，用一个已有的用户密钥发送请求。

容器通过其入口运行同一个子命令；见[容器部署](/zh-cn/deployment/docker/)。

## 回滚

v2 数据库以只读方式打开，从未被修改。要回退，停止 v3，在 v2 自己的数据目录上启动
v2 二进制。v3 store 可以删除，也可以留待再次尝试；由于标记的存在，再向其中导入
需要 `--merge` 或一个全新的目标。
