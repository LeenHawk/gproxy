---
title: "Usage, logs, and audit"
description: "Inspect costs, downstream and upstream requests, captures, and configuration changes."
---

The console separates usage, downstream requests, upstream calls, and audit records. These show cost, client traffic, actual upstream attempts, and configuration changes respectively.

## Usage and costs

A usage record represents one physical upstream call. Retries or failover can produce several records for one client request. Provider and credential attribution, tokens, media/tool quantities, and costs are stored independently of capture logs.

A missing token field may be `null`, not zero. `completeness` describes usage completeness, while `metrics` retains dynamic dimensions and protocol details. Costs come from configured pricing rules, not the upstream account balance.

Aggregates can filter by time, model, provider, credential, and other dimensions. Check `scanned` and `truncated`: a truncated scan is not a complete bill.

## Request logs

- **Downstream logs** describe client-to-gateway requests.
- **Upstream logs** describe physical upstream calls. One downstream request may have multiple attempts.
- **Captures** retain selected requests, responses, or stream events, subject to body-retention settings.

HTTP / SSE and WebSocket traffic have corresponding records. WebSocket frames are connection events; frame counts are not necessarily model-call counts.

## Enable body recording

Instance logging settings control upstream and downstream logs and bodies separately. For example:

```sh
curl -sS -X PATCH http://127.0.0.1:8787/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"logging":{"enableUpstreamLogBody":true}}'
```

Body recording adds storage and processing overhead. Log states distinguish uncaptured, truncated, and complete content. A missing body does not mean a request was never sent. Disabling capture does not disable usage settlement.

Default redaction masks recognized secret, Cookie, and token fields, but does not identify all personal or business information in a body. `disableLogRedaction` disables log redaction; configure it according to your needs.

## Query APIs

CLI / container HTTP management endpoints include:

| Path | Content |
| --- | --- |
| `/admin/api/usage` | Usage summary, groups, and trends |
| `/admin/api/usage/records` | Per-upstream-call usage records |
| `/admin/api/logs/downstream` | Downstream request list |
| `/admin/api/logs/upstream` | Upstream call list |
| `/admin/api/logs/downstream/{id}` | Downstream request details |
| `/admin/api/logs/captures/{id}` | Capture details |
| `/admin/api/audit` | Audit records |

Each endpoint enforces management scope and capabilities. Personal endpoints `/portal/api/usage`, `/portal/api/quota`, and `/portal/api/requests` restrict results to caller-visible data. Application queries through in-app IPC and does not expose these management endpoints over HTTP.

## Audit

Audit records cover management and OAuth actions, including actor, action, outcome, and time. `GPROXY_AUDIT_ENABLED=false` disables new audit entries while keeping history queryable. It does not disable request logs or usage settlement.

## Process logs

The CLI uses `GPROXY_LOG_FILTER` and `GPROXY_LOG_FORMAT` for log level and format, and writes process logs to stderr. Initial generated administrator credentials go to stdout. Service managers and containers may collect both streams, so protect the first startup log.

See [Pricing](/reference/pricing/) for cost rules and [Permissions, rate limits, and budgets](/guides/permissions/) for budgets.
