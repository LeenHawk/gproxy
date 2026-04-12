---
title: Graceful Shutdown
description: How GPROXY drains workers, flushes usage, and exits cleanly.
---

Graceful shutdown is implemented jointly by
[`apps/gproxy/src/main.rs`](https://github.com/LeenHawk/gproxy/blob/main/apps/gproxy/src/main.rs)
and
[`apps/gproxy/src/workers/mod.rs`](https://github.com/LeenHawk/gproxy/blob/main/apps/gproxy/src/workers/mod.rs).
The goal is simple: after `SIGINT` / `SIGTERM`, stop accepting new
requests, let in-flight work finish, flush background sinks, and exit
within a bounded time window.

## Sequence

1. The process listens for `Ctrl+C`. On Unix it also listens for
   `SIGTERM`.
2. Once shutdown is triggered, the Axum server enters the
   `with_graceful_shutdown` flow and **stops accepting new requests**.
3. The main thread calls `worker_set.shutdown()`, broadcasting the
   shutdown signal to all background workers.
4. `WorkerSet` waits up to **5 seconds** for workers to drain.
5. `UsageSink` closes its receiver, drains the remaining usage
   messages, and performs a **final batch write**.
6. `HealthBroadcaster` flushes any health states still in its debounce
   window to the database.
7. `QuotaReconciler` and `RateLimitGC` exit at the next loop iteration
   after receiving the signal.
8. If any workers have not finished within 5 seconds, the process
   logs a warning but does **not** block indefinitely — it exits so
   the orchestrator isn't left waiting on a stuck binary.

## Operational implications

- **Set a reasonable terminationGracePeriodSeconds.** On Kubernetes or
  systemd, give GPROXY at least 10 seconds to exit cleanly (5 s for
  worker drain + some headroom). Anything less risks clipping the
  final usage batch.
- **Hot reloading is not the same as a restart.** Most runtime
  settings (providers, models, users, permissions, rate limits,
  quotas) can be edited live from the console or admin API and take
  effect on the next request. You only need to restart for
  process-level things like changing the listen address via
  environment, swapping the database DSN, or upgrading the binary.
- **Don't SIGKILL on purpose.** A forced kill skips the usage drain,
  which means the last few requests won't be accounted for in `usages`
  or reconciled into `cost_used` until the next
  `QuotaReconciler` pass.

## Worker cheat sheet

| Worker | What it does on shutdown |
| --- | --- |
| `UsageSink` | Closes the receiver, drains queued messages, final batch write. |
| `HealthBroadcaster` | Flushes debounced health states. |
| `QuotaReconciler` | Exits the next loop iteration. |
| `RateLimitGC` | Exits the next loop iteration. |
