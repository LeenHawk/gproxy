---
title: 用量、日志与审计
description: "用量行与小时汇总、带分级捕获与脱敏的请求审计、保留策略、管理操作审计、请求 id 与进程日志"
---

GPROXY 把运维数据写入持久化后端，因此共用同一数据库的所有实例共享同一视图。
控制台在 **统计** 下读取这些数据：用量、管理操作与请求审计三个标签页。

## 请求 ID

每个网关请求都获得形如 `<instance>-<prefix>-<sequence>` 的 id：数字型
`GPROXY_INSTANCE_ID`（默认 `0`）、进程启动时随机选取的 64 位前缀，以及每进
程计数器，后两者均为十六进制。该 id 通过 `x-request-id` 响应头返回给客户端，
也是关联用量行、请求日志、上游捕获和门户最近请求列表的键。用量行的维度中还携
带 `instance_id` 与配置的 `instance_name`，因此共享数据库可以按实例拆分。

## 用量行与汇总

每次结算的交换都在同一批次内向 `usage_rows` 写入一行，并累加到
`usage_rollups` 的小时桶。所有到达上游的路径都会结算，包括服务界面；本地作
答的操作记录一条零成本行。

| 列 | 内容 |
| --- | --- |
| `request_id`、`at` | 关联 id 与结算的 Unix 时间。 |
| `provider_id`、`credential_id`、`upstream_model`、`operation` | 由谁服务了请求。 |
| `organization_id`、`team_id`、`user_id`、`user_key_id` | 谁被准入。 |
| `input_tokens`、`output_tokens`、`cached_input_tokens` | 一级 Token 计数；`input_tokens` 包含缓存读取。 |
| `metrics` | 维度数量，例如 `cache_creation_5m_tokens`、`reasoning_tokens`、`audio_seconds`、`web_searches`。 |
| `dimensions` | 限定符，例如 `service_tier`、`instance_id`、`instance_name`。 |
| `cost` | 结算的十进制成本。 |
| `usage_source` | Provider 报告了用量时为 `upstream`，GPROXY 自行计数时为 `estimated`。 |
| `ended` | `complete`，或客户端挂断、流中断时为 `interrupted`。 |
| `latency_ms` | 交换的墙钟时间。 |

汇总以小时、Provider、组织、团队、用户、上游模型和维度为键；概览趋势读取的
就是它。关闭 `enable_usage` 会停止持久化用量行；准入、结算与配额对账仍然运行。

### 查询用量

`GET /admin/api/usage?from&to` 在最长 366 天的范围内聚合用量行。`group_by` 可
为 `user_key`、`user`、`provider` 或 `model`；不指定时返回每个不同的维度组
合。过滤条件：`user_key_id`、`user_id`、`provider_id`、`credential_id`、
`model`。每行报告请求数、输入、输出与缓存 Token、5 分钟、30 分钟和 1 小时的缓
存写入以及成本。控制台的用量标签页提供同样的过滤条件加日期范围，并在
**用量与成本** 和 **配额窗口** 之间切换。`GET /admin/api/usage-trend?from&to`
返回每小时的数据点。

成本、Token 和维度回答的是不同的问题：成本是计价后的实际计费；Token 是
Provider 或估算器给出的计数；指标与维度承载 Token 之外的一切，由
`price_rates` 行计价（见[价格与分层](/zh-cn/reference/pricing/)）。

## 请求审计

请求审计保存下游交换（客户端发送与收到的内容）以及它引发的每一次上游尝试，
按请求 id 关联。捕获默认关闭，由设置页上的四个开关分级控制
（`PATCH /admin/api/log-settings` 或 `/admin/api/instance-settings`）。

| 开关 | 记录内容 |
| --- | --- |
| `enable_downstream_log` | 客户端的方法、路径、查询、IP、请求头、状态、错误类型、时长、每秒输出 Token。 |
| `enable_downstream_log_body` | 另加客户端的请求体与响应体。流式响应完整捕获，在流结束时写入。 |
| `enable_upstream_log` | 每次上游尝试：Provider、凭证、URL、方法、请求头、状态。 |
| `enable_upstream_log_body` | 另加上游的请求体与响应体。 |

只有设置了 `retention_days` 或 `max_database_size_mb` 才能打开请求体开关；
否则 API 返回 400。

`GET /admin/api/logs?start&end` 列出捕获的请求，过滤条件有 `user_id`、
`user_key_id`、`provider_id`、`status`、`request_id`，以及 `cursor` 和
`limit`（1 到 100，默认 50）。`GET /admin/api/logs/<request_id>` 返回下游记录
和按顺序排列的上游尝试。在控制台中，**请求审计** 在
`/admin/logs/<request_id>` 打开每条请求，请求头和请求体可复制；未捕获的字段
会明确标出。

客户端 IP 取对端地址；若对端是环回地址或列在 `GPROXY_TRUSTED_PROXIES` 中，则
取 `X-Forwarded-For` 的第一项或 `X-Real-IP`。

## 脱敏

脱敏默认开启，在存储之前作用于两个方向的请求头、查询串和请求体：

- 请求头 `authorization`、`proxy-authorization`、`x-api-key`、
  `x-goog-api-key`、`api-key`、`cookie` 与 `set-cookie` 变为 `[redacted]`。
- 名称属于已知机密的 JSON 字段以及表单或查询参数（`api_key`、`token`、
  `access_token`、`refresh_token`、`client_secret`、`password`、`code`、
  `signature`、`code_verifier`、`state` 等）被替换；嵌套对象与数组会被遍历。
- 超过 100 MiB 的请求体被截断并加标记。

`disable_log_redaction` 是显式的明文覆盖。控制台把它标红，因为此时凭证、
Cookie 和用户内容会按原样写入数据库。登录路径（`/oauth/*`、设备授权回调、
`/portal/api/login`、`/portal/api/password`）即便开启覆盖也始终脱敏。

## 保留与容量压力

清理每五分钟运行一次。

| 设置 | 效果 |
| --- | --- |
| `retention_days` | 删除早于截止时间的 `request_logs`、`wire_logs` 和 `usage_rows`。未设置时按 36,500 天处理。 |
| `max_database_size_mb` | 数据库超过上限时，每次清理各删除 `request_logs` 和 `wire_logs` 中最旧的 5,000 行。未设置时按 1,024 MiB 处理。 |

容量压力永远不会删除用量行或汇总；只有保留策略会，并且只按时间。数据库大小
在 SQLite 与 libSQL 上读取 `page_count × page_size`，在 PostgreSQL 上读取
`pg_database_size`，在 MySQL 上读取 `information_schema` 的合计。删除行本身不
会缩小 SQLite 文件。

## 管理操作审计

每次成功的管理 API 写操作都会写入一条 `audit_events` 行：操作者用户 id、动作
（例如 `providers.update`、`rule_preset.apply`、`credential.secret_reveal`、
`user_key.reveal`、`log_settings.update` 或 `channel_login.device_start`）、目标
类型与 id、时间以及客户端 IP。登录事件 `auth.setup`、`auth.login` 与
`auth.logout` 也会记录。读操作与配置导出不审计。

`GET /admin/api/audit?limit` 返回最新的事件（默认 100，最多 500）。
**管理操作** 标签页显示最新的 500 条，含操作者名称、IP、动作与目标，并支持文
本搜索。

## 凭证健康与配额周期

健康按凭证与上游模型跟踪，状态为 `healthy`、`degraded` 或 `dead`，并带有观察
到的状态码、详情字符串与时间。概览列出不健康的已启用凭证；凭证卡片显示按模型
的行并提供重置（`POST /admin/api/credentials/<id>/health-reset`）。重置只清除
记录的状态；仍在失败的上游会在下一次尝试时再次降级该凭证。

随响应返回或来自配额探测的上游配额窗口会持久化为凭证周期：窗口键与标签、周
期起止、已用与上限、边界是由上游报告还是推断得出、周期是开放还是已关闭。
`GET /admin/api/credential-cycles?from&to[&credential_id]` 列出它们；用量标签
页和概览的配额压力卡片（达到或超过 80% 的窗口）读取同一批数据。
`POST /admin/api/credentials/<id>/quota-probe` 按需刷新某个凭证的窗口。

## 进程日志

原生二进制通过 `tracing` 输出日志到标准输出。`GPROXY_LOG_FORMAT`
（`--log-format`）选择 `text`（默认）或按行分隔的 `json`。级别过滤来自
`RUST_LOG`，默认 `info`。清理、用量写入失败与捕获失败都会记录日志，并在有请求
id 时附带。
