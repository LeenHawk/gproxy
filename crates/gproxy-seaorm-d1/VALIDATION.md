# 独立 crate 验证记录

## Schema sync 与官方 migration

2026-09-15 北京时间 19:03:41–19:05:03，在真实 Cloudflare Worker / D1 上完成 13 个场景。
使用 SeaORM / sea-orm-migration 2.0.3、wasm-bindgen 0.2.127。以下验证直接调用当前 crate：

| 检查 | 结果 |
|---|---|
| 初始 sync | 创建父子两张表，外键实际生效 |
| 重复 sync | 初始与升级后的再次同步均无额外 DDL |
| 字段演进 | 重命名保留原数据，新增可空列正确 |
| 唯一索引 | 新增后拒绝重复值，移除后允许重复值 |
| sync SQL 失败 | 重复数据导致索引创建失败，D1 batch 撤销同批新增列，原有数据保留 |
| 额外列 | 保留 entity 未声明的既有列 |
| 官方 migrator | 分步 up、status、重复执行跳过已应用版本、官方历史表和 WASM 时间戳通过 |
| 失败与修复 | 失败版本不记为已应用；已提交的部分 DDL 保留，修复迁移后可继续执行 |
| down | 按逆序回退指定步数及全部步骤，并更新官方历史记录 |
| 交互式事务 | 手动 begin 导致迁移失败，不会写入虚假的成功版本记录 |

本地 14 项测试及原生/WASM Clippy `-D warnings` 通过。同步按 SeaORM SQLite 规则处理，
没有增加数据库版本检查、结构指纹或索引归属记录表。临时 Worker、D1 和本地一次性 harness
均已清理。英文与中文 README 的代码示例一致，Cargo 包清单包含两份 README。

该轮临时 Worker 的 WASM SHA-256：

```text
0c187077b36bba45f3baa30425d51cbcd811c0e74737f5a9e90c24646432def4
```

## 原始连接与 batch 验证

2026-09-15 北京时间 17:50:11–17:51:27，临时 Worker 直接依赖并执行本 crate，连接真实
Cloudflare D1。使用 SeaORM 2.0.3、wasm-bindgen 0.2.127，包含 60 秒新部署传播等待。
这轮执行没有使用原实验的 bridge 实现。

| 检查 | 结果 |
|---|---|
| Entity DDL / CRUD | 三张表建表，插入、查询、条件更新、删除通过 |
| 字段映射 | NULL、bool、i32 边界、JSON、毫秒时间戳、中文与 SQL 标点文本通过 |
| BLOB | 全部 256 种 byte 值与空 BLOB 正确读回 |
| 作用域与 CAS | 同名 key 隔离；成功更新 1 行、旧版本更新 0 行 |
| 并发 CAS | 16 个独立 HTTP 请求，1 个成功、15 个冲突；新请求读取 revision=1 |
| 安全整数 | `9007199254740991` 数值读写准确，超范围整数参数在执行前拒绝 |
| 精确整数 | 文本参数加 SQL CAST 保存 `9007199254740993`；文本投影正确读回，i64::MIN/MAX 也精确往返 |
| 原子 batch | 提交扣款后余额 900；重复账本唯一键失败使前一条扣款一并回滚，余额仍为 900 |
| 业务条件 | 更新 0 行不触发 batch 回滚，后续写入仍会提交；该演示记录随后删除 |
| batch 预检 | 后续语句方言非法时，第一条插入也未执行 |
| 顺序读取 | SQL 列顺序与别名字母序不同，显式 positional 投影仍按 SQL 顺序读取 |
| SQL 事务指令 | 原始 `BEGIN TRANSACTION` 被 D1 拒绝 |
| 清理 | 临时 Worker 与 D1 删除成功，并通过 API 确认不存在 |

编译和静态检查：8 项原生单元测试通过；原生与 `wasm32-unknown-unknown` Clippy
`-D warnings` 通过。workspace 的 WASM 检查使用 wasm-bindgen 0.2.128；云端打包使用本机
CLI 对应的 0.2.127。事务 API 的不可用性另以 WASM 编译失败检查验证。

临时 Worker 的 WASM SHA-256：

```text
c551f2d26f3420d73851d1f8f3c039f3cba290560cfd3b751ce5859c8ca21ce6
```

这一轮验证连接、字段映射及原子操作边界；sync/migration 的后续验证见上节。生产业务
entity、完整协议状态生命周期或吞吐量不在验证范围。按要求删除一次性 harness、部署脚本、生成 WASM 和实验构建目录，
保留库代码、单元测试与此摘要。
