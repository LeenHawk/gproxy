---
title: 从 v2 迁移到 v4
description: 用一个脚本将 v2 数据迁移到 v4，支持 SQLite、PostgreSQL 和 MySQL。
---

使用统一的 [migrate-v2-to-v4.py](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-to-v4.py)。源库和目标库都支持 SQLite、PostgreSQL、MySQL，可以跨数据库迁移。

脚本一次完成：**v2 源库 → SQLite 快照 → v3 SQLite → v4 SQLite → 目标库**。原库只读，账号、凭据、API key 和用量先交给现有升级器转换，再把完整的 v4 数据搬到目标库。

## 准备

停止旧实例并保留数据库备份。目标必须是**新的 SQLite 文件或空的 PostgreSQL/MySQL 数据库**；不要填写原库地址。加密实例继续使用原来的 `GPROXY_MASTER_KEY`。

准备 Python 3.10+、[v3.0.22](https://github.com/LeenHawk/gproxy/releases/tag/v3.0.22) 和 [v4](https://github.com/LeenHawk/gproxy/releases/latest) 可执行文件。v4 需要包含目标数据库驱动。纯 SQLite 不需要额外 Python 包；使用 PostgreSQL 或 MySQL 时安装对应驱动：

```sh
python3 -m pip install 'psycopg[binary]>=3,<4'  # PostgreSQL
python3 -m pip install 'PyMySQL[rsa]>=1,<2'     # MySQL
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-to-v4.py -o migrate-v2-to-v4.py
```

## 执行一次迁移

SQLite 示例：

```sh
python3 migrate-v2-to-v4.py \
  --source ./old-data/gproxy.db \
  --target ./new-data/gproxy.db \
  --v3-bin ./v3/gproxy \
  --v4-bin ./v4/gproxy
```

把 `--source`、`--target` 换成 `postgresql://user:password@host/database` 或 `mysql://user:password@host/database` 即可使用远程数据库；两端可以不同。含密码的连接串可通过 `GPROXY_SOURCE_DSN`、`GPROXY_TARGET_DSN` 环境变量提供，并省略对应参数。需要加密密钥时也通过环境变量设置：

```sh
read -rsp '源数据库连接串：' GPROXY_SOURCE_DSN; echo; export GPROXY_SOURCE_DSN
read -rsp '空目标数据库连接串：' GPROXY_TARGET_DSN; echo; export GPROXY_TARGET_DSN
read -rsp '原主密钥（明文源库留空）：' GPROXY_MASTER_KEY; echo; export GPROXY_MASTER_KEY
python3 migrate-v2-to-v4.py --v3-bin ./v3/gproxy --v4-bin ./v4/gproxy
```

脚本自动创建工作目录并打印位置；可用 `--work-dir` 指定一个尚不存在的目录。默认在 PATH 中查找 `gproxy-v3` 和 `gproxy`，找到时可省略二进制路径参数。PostgreSQL 源表不在 `public` 中时使用 `--source-schema`；目标 PostgreSQL 使用 `public`。MySQL 源表需要 InnoDB，可在 DSN 中使用 `ssl-mode`、`ssl-ca`、`ssl-cert`、`ssl-key` 设置 TLS。

## 完成后

中间库和每阶段报告全部保留在工作目录。脚本检查账号、API key 摘要、历史费用，并对最终 PostgreSQL/MySQL 搬运逐表核对行数和行内容校验和。检查 `v3-import.log`、`v4-import.log` 和 `*-verification.json` 后，让 v4 使用目标库及原主密钥，再恢复流量。

升级边界沿用现有转换规则：部分旧权限和规则需要重配；历史用量明细保留，已删除的明细不能从汇总重建；旧请求日志、会话、审计和缓存不会变为 v4 对应记录。v4 金额保留 9 位小数，精确历史费用仍保存在 `metrics.v3.cost`。参见 [v2 → v3](/zh-cn/deployment/v2-to-v3/)和 [v3 → v4](/zh-cn/deployment/v3-to-v4/)。

失败会停止并保留现场，不删除原库或自动清空目标库。排查后使用新的工作目录和空目标重试。工作目录包含数据库和秘密，应保持私有；v3.0.22 的密钥参数会短暂出现在进程参数中，不要使用 shell 跟踪。
