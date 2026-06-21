---
title: Observability
description: 理解 v2 的 usage、request log、audit log、credential health、settings、retention 和 Prometheus metrics。
---

v2 把运维数据写入 persistence backend，这样 native、多实例和 edge 部署可以共享同一视图。热路径设置加载在 control-plane snapshot 中。

## Request ID

每个网关请求都会生成 request id。Usage、downstream log、upstream log 都带这个 id，因此可以在表和 console 视图之间关联一次调用。

## Usage

Usage 记录包含：

- request id 和时间；
- route name、provider id、credential id；
- org、team、user、user key id；
- operation 和 kind；
- model；
- input、output、cache-read、cache-creation tokens；
- cost；
- latency 和 usage source。

Usage 由 `instance_settings.enable_usage` 控制，默认开启。结算还会更新 quota 和 token-limit counter。

## Request Logs

请求日志分为 downstream 和 upstream 两条流：

| Setting | 捕获内容 |
| --- | --- |
| `enable_downstream_log` | 面向客户端的 method、path、query、status、headers。 |
| `enable_downstream_log_body` | Downstream request 和 response body。 |
| `enable_upstream_log` | Provider URL、method、status、latency、headers。 |
| `enable_upstream_log_body` | Upstream request 和 response body。 |

默认开启 redaction。`disable_log_redaction` 可用于调试，但可能暴露 secret，不应随意开启。

## Audit Logs

Admin 和 portal 的 mutation path 会写 audit row，包含 actor id/name、action、target、status 和 source IP。它用于回答“谁改了控制面”，而不是调试 LLM payload。

## Credential Health

Credential status 行跟踪每个 credential/channel pair：

- `health_kind`；
- 可选结构化 `health_json`；
- `checked_at`；
- `last_error`。

Pipeline 和 channel response classifier 决定 credential 应该重试、cooldown，还是视为 auth-dead。Console 通过 `/admin/credential-statuses` 展示当前状态。

## Metrics

`/metrics` 需要 admin，并从持久化聚合数据渲染 Prometheus text，不使用进程本地 counter。当前指标包括：

- `gproxy_requests_total`
- `gproxy_tokens_total`
- `gproxy_upstream_latency_ms`
- `gproxy_credential_health`
- `gproxy_quota_total`
- `gproxy_quota_used`

这种设计让 native 多实例和 edge 部署中的 metrics 仍然有全局意义；进程本地 counter 在这些场景里会误导。

## Retention

`instance_settings.retention_days` 控制 usage 和 request-log row 清理。`None` 或非正数表示永久保留。Retention 只应清理 logs 和 usage 数据，不应删除业务/控制面记录。
