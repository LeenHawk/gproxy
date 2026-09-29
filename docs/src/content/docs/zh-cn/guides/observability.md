---
title: "用量、日志与审计"
description: "查看费用、上下游请求、抓包与配置变更记录。"
---

控制台把用量、下游请求、上游调用和审计分开显示。它们回答不同的问题：花了多少、客户端发送了什么、实际调用了哪个上游，以及谁修改了配置。

## 用量与费用

用量记录对应一次实际的上游调用。一次客户端请求发生重试或故障切换时，可能产生多条用量记录。供应商和凭证归属、token 数、媒体与工具数量、费用都可独立于抓包保存。

单条记录中，未报告的 token 字段可能是 `null`，不等于 0。`completeness` 表示用量的完整程度；`metrics` 保留动态维度与协议细节。费用由配置的价格规则计算，不是上游账户余额。

汇总支持按时间、模型、供应商、凭证等条件查询。`scanned` 与 `truncated` 表示扫描数量和是否触及上限；截断的结果不能当作完整账单。

## 请求日志

- **下游日志**记录客户端与网关之间的请求。
- **上游日志**记录网关实际发出的调用。同一个下游请求可关联多次上游尝试。
- **抓包内容**保存选定的请求、响应或流事件，是否保留正文由设置决定。

HTTP / SSE 与 WebSocket 都有对应的记录。WebSocket 帧属于连接上的事件，不应简单把帧数当成模型调用次数。

## 启用正文记录

在实例日志设置中分别控制上游、下游日志与正文。API 示例：

```sh
curl -sS -X PATCH http://127.0.0.1:8787/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"logging":{"enableUpstreamLogBody":true}}'
```

正文记录会增加存储和处理开销。日志状态会区分未捕获、截断和完整内容；没有正文不代表请求没有发送。关闭抓包也不等于关闭用量结算。

默认脱敏会遮蔽识别到的密钥、Cookie 和令牌字段，但不会识别正文中的所有个人或业务信息。`disableLogRedaction` 会关闭日志脱敏，应按实际需要配置。

## 查询 API

CLI / 容器的 HTTP 管理接口提供：

| 路径 | 内容 |
| --- | --- |
| `/admin/api/usage` | 用量汇总、分组与趋势 |
| `/admin/api/usage/records` | 逐次上游调用的用量记录 |
| `/admin/api/logs/downstream` | 下游请求列表 |
| `/admin/api/logs/upstream` | 上游调用列表 |
| `/admin/api/logs/downstream/{id}` | 下游请求详情 |
| `/admin/api/logs/captures/{id}` | 抓包详情 |
| `/admin/api/audit` | 审计记录 |

各接口会检查管理范围与能力。个人接口 `/portal/api/usage`、`/portal/api/quota` 和 `/portal/api/requests` 限定在调用方可见的数据。Application 通过应用内 IPC 查询，不在 HTTP 端口开放这些管理接口。

## 审计

审计记录管理与 OAuth 操作，包括操作者、动作、结果和时间。`GPROXY_AUDIT_ENABLED=false` 可关闭新增审计，已有记录仍可查询；它不关闭请求日志或用量结算。

## 进程日志

CLI 使用 `GPROXY_LOG_FILTER` 和 `GPROXY_LOG_FORMAT` 设置日志级别与格式，进程日志写入标准错误。首次初始化生成的管理员信息写入标准输出。服务管理器和容器可能同时收集这两路输出，请妥善保管首次启动日志。

费用规则见[价格与分层](/zh-cn/reference/pricing/)，预算见[权限、限流与费用预算](/zh-cn/guides/permissions/)。
