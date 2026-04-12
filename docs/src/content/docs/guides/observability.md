---
title: Observability
description: Usage accounting, request logging, and health tracking in gproxy.
---

GPROXY captures three independent streams of operational data: **usage**,
**request logs**, and **health**. Each is controlled by its own toggle and
written through its own background worker so the hot path stays fast.

## Usage accounting

When `enable_usage = true` (global setting or TOML `[global]` block),
every completed request produces a usage record with at least:

- User, provider, channel
- Route kind (chat / responses / messages / generateContent / …)
- Model id and alias (if one was used)
- Input / output / total tokens
- Cost (USD), looked up via alias-first pricing

Records are pushed into a channel and drained by the `UsageSink` worker,
which writes them to storage in **batches** so high-throughput instances
don't serialize on the database.

On shutdown, the sink drains its buffer and performs a final batch write
before the worker exits — see [Graceful Shutdown](/reference/graceful-shutdown/).

The console's usage dashboards and the `/admin/usages` endpoints read
from this table.

## Upstream and downstream logging

Two independent toggles control request logging:

| Flag | Captures |
| --- | --- |
| `enable_upstream_log` | The upstream HTTP envelope (URL, status, headers, timing). |
| `enable_upstream_log_body` | Also capture the upstream request / response **body**. |
| `enable_downstream_log` | The downstream (client-facing) HTTP envelope. |
| `enable_downstream_log_body` | Also capture the downstream request / response **body**. |

Headers are sanitized according to the global **sanitize rules** before
being stored, so API keys and other secrets don't leak into the log
table. Body capture is intentionally expensive — leave it off in
production unless you're actively debugging.

The console exposes both streams under *Observability → Requests*, with
filters by user, provider, model, status, and time range.

## Health tracking

For every credential on every provider, GPROXY maintains an in-memory
health state:

- **Healthy** — the default, counts towards load balancing.
- **Cooldown** — temporarily skipped after a retryable failure (for
  example, upstream rate limiting or transient 5xx).
- **Disabled** — skipped until the admin re-enables, typically after
  an auth error like 401 / 403.

The `HealthBroadcaster` worker **debounces** these updates — a burst of
failures turns into one write — and persists them so the console can
show the current state after a restart.

## Where the data lives

All three streams land in the configured database (SQLite by default).
There is no external sink dependency — point a SQL client at your DSN
and you can query everything GPROXY records.

Typical tables you'll want to know about:

- `usages` — one row per completed request, aggregated through the sink.
- `upstream_requests` / `downstream_requests` — envelopes and optional
  bodies, when the corresponding flag is set.
- `provider_health` — latest known state per credential.

## Rotating and pruning

GPROXY does not include a log-rotation feature. Because everything is in
the database, handle retention with a scheduled SQL job — for example,
`DELETE FROM upstream_requests WHERE created_at < NOW() - INTERVAL '14 days'`
on PostgreSQL. The `QuotaReconciler` only reads usage rows to recompute
`cost_used`; pruning old usage is safe as long as you keep whatever window
you need for billing audits.
