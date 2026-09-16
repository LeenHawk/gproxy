# 独立 crate 验证记录

## OAuth 分层白名单与 JSON 匹配（本地验证）

2026-09-17：两个 crate 共 43 项原生测试通过；原生及 WASM all-targets Clippy
`-D warnings` 通过。新增 JSON 匹配测试验证顶层字符串、大小写、空白、特殊字符／绑定参数、
NULL、空数组、非数组及嵌套／非字符串元素。PostgreSQL／MySQL 只验证 SQL 构造和参数绑定，
没有真实数据库执行证据；MySQL 表达式使用 8+ 的 JSON_TABLE。当前开源 SeaORM 2.0.3
没有 MSSQL 后端，未集成单独的 SeaORM X；未实现后端明确报错，不返回伪造的匹配结果。

实际 `Store<D1Connection>` WASM 连接 Miniflare 本地 D1，两组新增场景通过：

- 全局上限，同级已配置名单取并集、跨级取交集，NULL 继承、空数组、用户限制、无关组织
  隔离、团队所属组织继承（不依赖额外组织成员行）、成员关系变动和客户端停用。
- 策略拒绝不批准设备、不创建 key/grant/code、不消费 code／refresh token、不更新刷新统计；
  收紧策略后已有 access token 校验失败，恢复允许后未消费的源 token 可以正常换取。

此轮验证包含完整建表和实际 D1 batch，不只是 SQL 生成。未部署云端 Worker、未访问生产
数据库、未执行既有数据库迁移。临时 harness 使用 CLI 对应的 wasm-bindgen 0.2.127，
workspace 静态检查使用 0.2.128；临时 harness／构建目录验证后删除，永久回归测试保留。

## 精确金额与 Store 接入（本地验证）

2026-09-17：两个 crate 共 39 项原生测试通过（适配器 22 单元 + 6 batch + 2 定点数；
Store 1 连接配置 + 8 集成），原生及 WASM all-targets Clippy `-D warnings` 通过。

`FixedDecimal` 原生 SQLite 验证 i64::MIN/MAX、超过 2^53 的精确往返、实际列值
`typeof = integer`、NULL、数值排序／过滤、JSON 字符串、精度与溢出拒绝，以及中点取偶。
过长十进制尾数使用精确解析，防止先被 Decimal 解析器舍入后错误接受。PostgreSQL／MySQL
仅断言生成的 CAST SQL，没有真实数据库执行证据。

当前 WASM 代码连接 Miniflare 5.20260825.0-alpha 本地 D1 binding，金额场景验证完整 i64
范围、超过 2^53、NULL、数值过滤及整数递增。另通过实际 `Store<D1Connection>` 完成以下
7 组场景，每组使用独立数据库并执行完整 schema：

- 批量 CRUD、默认值／缺失／重复 ID、条件修改／删除、count/page、设置单例、唯一键回滚。
- 改写规则原子替换、启用筛选、有序加载和配置快照。
- 精确结算、防重复扣款、重试金额不一致和整数溢出时整批回滚。
- 协议状态 CAS、过期重建、旧版本拒绝和删除。
- OAuth code／refresh 单次消费、统计不重复及撤销后失效。
- agent 分配预留／启用、旧版本拒绝、切换后保留历史资源目标。
- 设备批准与 key/grant/code 原子创建、重复批准拒绝、创建失败回滚、客户端退役与重启后旧授权仍失效。

批量主键边界使用 106 个 ID，实际发现 D1 表达式深度上限 100：即使参数不超限，连续 OR
仍失败。单主键读取改用 IN 并按参数数目分块，同一次 batch 完成；修复后上述 D1 场景和
原生 Store 测试重新通过。复合主键继续按主键宽度控制每条条件的参数数目。

原生另测试同一窗口的两个并发结算及凭证 CAS 只有一个首次写入胜者；连接池只有一个
SQLite 连接，不代表多进程或多服务器压力测试。本次未部署 Cloudflare Worker，未操作
生产数据库，也未执行既有 schema 数据迁移。临时 harness 使用与已装 CLI 匹配的
wasm-bindgen 0.2.127；workspace WASM 静态检查保持 0.2.128。一次性 harness 与其构建
目录在验证后删除，永久回归测试保留在两个 crate 中。

## 自动实体／关联投影（本地验证）

2026-09-16：22 项单元测试和 6 项 SQLite 批量集成测试通过，原生及 WASM all-targets
Clippy `-D warnings` 通过。新增测试使用 ORM 生成的真实 SQLite JOIN 结果，经过 D1
对象结果解码器，再由 SeaORM 的可选关联 model selector 还原模型；与原生关联结果对比。
覆盖改名列、ActiveEnum、LEFT JOIN 全 NULL、INNER JOIN、一对一／一对多／多对多、
三表别名、重复选择列、聚合／自定义别名预检，以及仅选择实体中受支持字段的子集。

另将当前代码编为 WASM，连接 Miniflare 本地 D1，通过 8 个场景：单实体／枚举、
一对一空关联、一对多分组、INNER JOIN、多对多分组、三表关联、独立的批量关联投影、
列子集与 LIMIT/OFFSET。另新增原生条件查询回归，验证 AND／OR、IN、LIKE、比较、
NULL 关联筛选、参数绑定及 EXISTS，并通过 D1 对象结果解码器还原。未访问真实 Cloudflare 或生产数据库。临时 harness 使用与本机
CLI 匹配的 wasm-bindgen 0.2.127；workspace 的 WASM 静态检查使用 0.2.128。
临时目录在验证后删除，保留 crate 内可重复执行的回归测试。

## 统一批量接口（本地验证）

2026-09-16：16 项字段映射／schema 单元测试、6 项真实 SQLx SQLite 集成测试通过，
原生及 WASM all-targets Clippy `-D warnings` 通过。SQLite 验证批量增删查改、
逐项结果顺序、空结果、混合写后读、RETURNING、SQL 失败回滚、零行更新及依赖条件。

另用当前 crate 构建 WASM，通过 Miniflare 5.20260825.0-alpha 的本地 D1 binding 执行
13 项场景：空批次、批量 CRUD、有序查询、空结果、BLOB／JSON、混合批次、RETURNING、
回滚、零行条件、方言预检、投影模式预检、参数上限预检、提交后解码失败。全部通过。
该验证使用本地 D1 运行时，未部署真实 Cloudflare Worker，也未访问生产数据库。
临时 harness 为匹配已安装的 wasm-bindgen CLI 单独锁定 0.2.127；workspace 的 WASM
检查仍使用 0.2.128。未更改 workspace 的 wasm-bindgen 版本。临时目录验证后删除。

SeaORM 2.0.3 的 proxy + SQLx SQLite 组合存在上游编译错误，因此 proxy 改为只在 WASM
启用，原生 DDL 规划使用 mock 回执，真实 SQLite 事务测试走 SQLx。PostgreSQL／MySQL
复用原生事务实现，但本次没有运行它们的真实数据库测试。

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
