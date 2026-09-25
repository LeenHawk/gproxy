---
title: 从 v3 迁移到 v4
description: 使用原配置启动 v4，自动迁移已有的 v3 SQLite 数据库。
---

**替换二进制，按原来的配置启动即可。** v4 会识别已有的 v3 SQLite 数据库并自动迁移。
继续使用原数据目录、数据库路径和 `GPROXY_MASTER_KEY`，不需要导出、新建目录或
重设密码。原用户名、密码和 API key 保持可用。

## 启动时做什么

绑定 HTTP 端口前，v4 先生成完整 SQLite 快照，包括已提交的 WAL 内容，再在原库旁的
临时数据库中完成转换。使用已有主密钥解密凭证，保留用户的 Argon2 密码哈希。只有
导入和运行快照装配全部成功，才替换原数据库路径，并在旁边保留
`gproxy.db.v3-*.bak` 备份，日志会输出备份位置。

转换失败不会替换原库。修正实际错误（例如恢复原有主密钥）后重启即可。替换前中断，
原路径仍然是 v3；替换后启动，通过 v4 的迁移账本识别为已完成，不会重复导入。

与其他 schema 迁移一样，升级期间保持单写者：停止 v3 后再启动 v4。即使数据库已经
分开，也不要让两套部署同时使用同一份 OAuth 凭证，以免 refresh token 轮换互相影响。

## 数据如何处理

| 数据 | 结果 |
| --- | --- |
| Provider 与凭证 | 转换已知渠道名称和配置，按当前主密钥重新封装凭证。 |
| 模型路由与成员 | 原公开名称合入路由名称；额外名称展开为独立路由并复制成员。 |
| 用户、密码、API key、组织与团队 | 保留密码哈希和密钥认证摘要。 |
| 价格与预算 | 转为 v4 单位；超过 9 位小数时复用 Store 的最近偶数舍入，并报告变化。 |
| 权限、限流与改写 | 迁移支持的表达形式，报告语义变化和未能映射的内容。 |
| 用量、抓包、请求日志、会话与审计历史 | 保留在 v3 备份中，不复制到 v4 运行表；原浏览器会话需要重新登录。 |

没有可转换渠道或配置的 Provider 会连同其关联行一并跳过，并在报告中列明。
这不包括忽略数据库读写错误或错误的解密密钥。启动后查看报告中的渠道遗漏和权限变化。

`gproxy migrate` 也使用相同的自动升级路径。升级完成后由 SeaORM 的
`seaql_migrations` 账本管理 v4 schema 变更。`migrate --status` 只查询状态，不执行
v3 升级。

## 可选：另行导入备份或导出文件

如果确实要导入另一份实例，仍可显式执行：

```sh
gproxy --data-dir ./other-instance migrate
gproxy --data-dir ./other-instance import --from-v3 /path/to/v3-backup.db
```

这个显式命令读取加密源时，通过 `--source-master-key` 或
`GPROXY_IMPORT_SOURCE_MASTER_KEY` 提供源主密钥。目标必须为空，或只包含先前未完成
的同一导入。加 `--skip-unmappable-providers` 使用与自动启动相同的跳过策略。

也支持 v3 的 `POST /admin/api/export` JSON，导出时使用
`{"include_secrets": true}`。与直接读取 SQLite 不同，这种导出缺少密码哈希、权限、
限流和 Provider 模型。导入没有哈希的 JSON 时可用 `--admin-password` 设置控制台
密码；直接迁移 SQLite 不需要它。
