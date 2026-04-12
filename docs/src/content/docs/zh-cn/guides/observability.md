---
title: 可观测性
description: gproxy 中的用量记账、请求日志与健康状态追踪。
---

gproxy 捕获三条互相独立的运维数据流：**用量 (usage)**、**请求日志** 与
**健康状态 (health)**。每一条都有各自的开关，并由各自的后台 worker 写入，
保证热路径不受影响。

## 用量记账

当 `enable_usage = true` 时 (全局设置或 TOML 的 `[global]` 块)，
每一次完成的请求都会产生一条用量记录，至少包含：

- 用户、供应商、通道
- 路由类型 (chat / responses / messages / generateContent / …)
- 模型 id 及别名 (若使用)
- 输入 / 输出 / 总 token
- 成本 (USD)，先按别名查价，未命中再按真实模型查

记录被推入 channel，由 `UsageSink` worker 消费并**批量**写入存储，
避免高并发场景下数据库成为瓶颈。

关机时 sink 会排空缓冲、再写最后一批后退出 —— 详见
[优雅关机](/zh-cn/reference/graceful-shutdown/)。

控制台的用量仪表盘与 `/admin/usages` 接口均读自这张表。

## 上下游日志

请求日志由两对独立开关控制：

| 开关 | 捕获内容 |
| --- | --- |
| `enable_upstream_log` | 上游 HTTP 信封 (URL、状态、header、耗时)。 |
| `enable_upstream_log_body` | 同时捕获上游请求/响应 **body**。 |
| `enable_downstream_log` | 下游 (面向客户端) HTTP 信封。 |
| `enable_downstream_log_body` | 同时捕获下游请求/响应 **body**。 |

存入前 header 会根据全局的**脱敏规则 (sanitize rules)** 过滤，避免 API key
等机密泄漏到日志表。body 捕获代价较高 —— 生产环境建议仅在排障时开启。

控制台在*可观测性 → 请求*中提供了这两条流，可按用户、供应商、模型、状态、
时间段过滤。

## 健康状态

gproxy 为每个供应商的每个凭证在内存中维护健康状态：

- **Healthy** —— 默认，参与负载分配。
- **Cooldown** —— 在一次可重试失败后暂时跳过 (如上游限流或短暂 5xx)。
- **Disabled** —— 跳过直到管理员重新启用，通常在出现 401/403 鉴权错误时触发。

`HealthBroadcaster` worker 会对这些更新做 **debounce** —— 一阵失败只写一次
数据库 —— 并持久化，重启后控制台仍能看到当前状态。

## 数据落在哪里

以上三条流全部落在配置的数据库中 (默认 SQLite)。不依赖任何外部系统 ——
用 SQL 客户端就能查询。

你大概率关心的表：

- `usages` —— 每一次完成请求一行，由 sink 聚合。
- `upstream_requests` / `downstream_requests` —— 请求信封与可选 body，取决于开关。
- `provider_health` —— 每个凭证最近的健康状态。

## 轮转与清理

gproxy 不自带日志轮转。因为数据都在数据库里，直接用定时 SQL 作业处理即可 ——
比如在 PostgreSQL 上执行
`DELETE FROM upstream_requests WHERE created_at < NOW() - INTERVAL '14 days'`。
`QuotaReconciler` 只会读用量行来重算 `cost_used`；只要保留账单审计所需窗口，
清理旧的用量是安全的。
