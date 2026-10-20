---
title: 从 v2 PostgreSQL 迁移到 v4
description: 使用离线迁移工具，将 v2 PostgreSQL 数据迁入新的 v4 PostgreSQL 数据库。
---

下载同一版本的 [Shell 脚本](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-postgres.sh)和 [Python 辅助程序](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-postgres.py)，将两个文件放在同一目录。工具针对 v2.3.2 的表结构，按以下路径离线迁移：

```text
v2 PostgreSQL → 临时 v2 SQLite → v3.0.22 SQLite → 新的 v4 PostgreSQL
```

源 PostgreSQL 数据在同一个只读、可重复读事务中读取，脚本不修改原库。快照中的凭据保持加密，由现有 v3/v4 导入器使用原主密钥解密并重新加密。两个步骤都不会启动 HTTP 服务，脚本也不会自动下载或安装 GPROXY 二进制。

SQLite 文件只用作导入中间文件，不能代替 `pg_dump` 备份：它不会重建 PostgreSQL 的索引、默认值和约束供原库恢复使用。

## 下载与准备

先将脚本保存到本地：

```sh
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-postgres.sh -o migrate-v2-postgres.sh
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-postgres.py -o migrate-v2-postgres.py
```

需要准备：

- Bash、带 SQLite 支持的 Python 3.10+，以及 psycopg 3：

  ```sh
  python3 -m venv .migration-venv
  .migration-venv/bin/pip install 'psycopg[binary]>=3,<4'
  export PYTHON="$PWD/.migration-venv/bin/python"
  ```

- 适合当前平台的 **v3.0.22** 和当前稳定版 v4 可执行文件，分别下载、解压到不同目录：[v3.0.22](https://github.com/LeenHawk/gproxy/releases/tag/v3.0.22)、[稳定版](https://github.com/LeenHawk/gproxy/releases/latest)。v4 必须包含 PostgreSQL 驱动。v3 固定使用 3.0.22，因为脚本适配的是该版本的源数据结构和命令参数。官方 v4.0.2 Linux CLI 缺少 PostgreSQL 驱动，请使用新版稳定 CLI；本工具已用 v4.1.8 验证。
- 尝试升级前的完整 `pg_dump` 备份、原主密钥，以及足够保存两份 SQLite 数据库的本地磁盘空间，包括请求日志。v3 导入器会把用量明细读入内存，需要按历史记录数量预留内存。
- 一个专用、全新且为空的 PostgreSQL 目标数据库，使用 `public` schema。导入前不要对它启动 GPROXY；导入器会创建表。

最终迁移前，停止所有向源库写入的 GPROXY 实例。建议先把原始 v2 备份恢复到临时 PostgreSQL 数据库，再使用只有 SELECT 权限的账号读取。如果已经让 v4 打开过旧库，请使用升级前的 v2 备份，不要使用混合了新旧表的数据库。不需要把数据库、凭据或主密钥发送给他人。

## 1. 准备并检查中间库

在 shell 中设置源连接地址和原来的 Base64 格式 `GPROXY_MASTER_KEY`。下面的交互输入方式不会把这些值写入 shell 历史：

```sh
read -rsp '源 PostgreSQL URL：' GPROXY_V2_SOURCE_DSN; echo
export GPROXY_V2_SOURCE_DSN
read -rsp '原主密钥（明文源库留空）：' GPROXY_MASTER_KEY; echo
export GPROXY_MASTER_KEY

# v2 表不在 public 中时，可指定源 schema：
# export GPROXY_V2_SCHEMA=your_schema

bash migrate-v2-postgres.sh prepare ./migration-work /absolute/path/to/v3/gproxy
```

`migration-work` 必须尚不存在。脚本会以私有权限创建它，所有输出都保存在里面。脚本分批复制源 schema 中的全部表，核对行数，仅在 SQLite 中补齐旧结构缺少的字段，然后执行 v3 预检、导入、完整性和用量检查。旧 `image_price` 保留为按图片张数计费的 `image_outputs` 价格，同时保留每百万 token 的价格，不会直接改名成图片 token 价格。

继续之前，请检查：

- `snapshot-report.json`：源表行数和临时结构调整。
- `v3-preflight.log`、`v3-import.log`：转换结果、跳过的表、权限和路由变化。
- `v3-verification.json`：主要配置数量和历史费用精确总额。

整个目录包含数据库副本和报告，应保持私有。v3.0.22 只能通过 `--from-v2-master-key` 接收源主密钥，因此密钥会短暂出现在 v3 进程参数中。请在可信机器上运行，不要使用 `bash -x`；脚本自身不会打印密钥。

## 2. 导入新的 PostgreSQL 数据库

继续使用相同的主密钥和已经准备好的目录：

```sh
read -rsp '新的空目标 PostgreSQL URL：' GPROXY_V4_TARGET_DSN; echo
export GPROXY_V4_TARGET_DSN

bash migrate-v2-postgres.sh import ./migration-work /absolute/path/to/v4/gproxy
```

脚本先核对密钥指纹并确认目标为空，再调用 v4 的 `import --from-v3` 导入配置、身份和用量。随后检查主要实体 ID、密码哈希、API key 摘要、用量行数、原请求 ID 和费用，包括 v4 舍入后的实际存储金额。查看 `v4-import.log` 和 `v4-verification.json`。无法转换的供应商会使显式导入失败；脚本没有启用跳过供应商的选项。

检查报告后，再将 v4 部署指向新数据库，继续使用原主密钥。恢复流量前确认旧账号登录、API key、上游凭据、路由、权限和历史用量正常。不要让两套部署同时使用同一份 OAuth refresh token。

## 迁移范围

工具复用现有迁移规则，不保证每个 v2 功能的行为完全相同。供应商、凭据、路由、用户、可恢复的 API key、密码哈希和保留的用量明细按这些规则转换。详细边界参见 [v2 → v3 说明](/zh-cn/deployment/v2-to-v3/)和 [v3 → v4 说明](/zh-cn/deployment/v3-to-v4/)。

- v4 存储金额时保留 9 位小数，原始用量费用保存在 `metrics.v3.cost`，校验使用该精确值。
- 已删除的用量明细无法从旧聚合数据重建。v2 用量汇总和历史配额周期保留在快照中，不会导入。
- 请求日志会进入临时 v3 数据库，但不会变成 v4 抓包。会话、审计日志、凭据健康状态和缓存不会迁入。
- 部分权限、限流、改写和供应商设置会变化或无法转换。请阅读两阶段的报告，并在恢复服务前重新配置必要策略。数量校验通过不代表策略完全等价。

## 失败、重试与回退

命令失败时返回非零退出码并保留所有工作文件。脚本不会删除数据库，也不会自动撤销部分导入。`prepare` 不需要目标连接，两个步骤都不会写入源 PostgreSQL 数据库。

准备失败时，检查报告、修复原因，再使用**新的工作目录**重试。导入失败时，保留部分写入的目标库和日志以便排查；修复原因后，使用**新的空目标库和新的工作目录**重新完整执行。脚本拒绝复用非空目标，不要用 `--merge` 绕过检查。

切换部署前，可继续使用原部署和原数据库。切换后如需回退，停止 v4 并恢复原 v2 部署和备份；v4 中新产生的用量不会自动合并回 v2。保留原始备份和工作目录，直到迁移核对完成且符合备份保留策略。
