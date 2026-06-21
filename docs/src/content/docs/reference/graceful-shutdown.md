---
title: Graceful Shutdown
description: Native shutdown signals, request draining, stream settlement, and what is not currently guaranteed.
---

The native GPROXY v2 server uses Axum's `with_graceful_shutdown` around the
top-level HTTP server. It listens for:

- `Ctrl+C` / SIGINT on all native platforms supported by Tokio;
- SIGTERM on Unix.

When either signal arrives, the server stops accepting new connections and lets
Axum drive in-flight services to completion.

## Native sequence

1. The binary builds persistence, cache, the control-plane snapshot, HTTP
   client, channel registry, and router.
2. The server binds the configured address.
3. `axum::serve(...).with_graceful_shutdown(shutdown_signal())` waits for a
   shutdown signal.
4. On signal, GPROXY logs `shutdown signal received`.
5. Axum graceful shutdown stops accepting new work and waits for in-flight
   request futures according to Axum/Tokio behavior.
6. When the server future completes, `main` returns.

There is no documented fixed drain timeout in the current native serve path.
Your service manager's termination grace period is therefore the outer bound.
Avoid sending SIGKILL unless you are willing to interrupt in-flight requests.

## Streams and billing settlement

The HTTP server shutdown path is separate from request settlement. For
successful content-generation attempts:

- full responses settle inline;
- native streaming responses are wrapped by a guard;
- normal stream end settles as `Complete`;
- upstream interruption or client drop settles as `Interrupted` via the guard;
- settlement is exactly-once for the wrapped stream.

Settlement refunds any pending quota estimate and writes actual usage/cost when
it can extract or count usage. If the process is killed before a settlement task
can run, the pending quota estimate self-heals through its 15-minute cache TTL,
but the killed request may not produce a final `usages` row.

## Background tasks

The serve path can spawn background work such as:

- Redis invalidation listener for multi-instance config invalidation;
- retention cleanup for usage and request-log rows;
- tokenizer download behavior when enabled by instance settings.

These tasks are not exposed as a separate worker set with a documented
application-level drain deadline in the current code. Treat process shutdown as
HTTP graceful shutdown plus best-effort background task cancellation by runtime
drop.

## Operational guidance

- Prefer SIGTERM from systemd, Docker, Kubernetes, or the host supervisor.
- Give the process a reasonable grace window so in-flight upstream calls and
  stream settlement can finish.
- Do not rely on environment changes for live config. Most provider, route,
  authz, pricing, and rule changes should be made through the console/admin API
  and do not need a restart.
- Restart for process-level settings such as bind address, persistence backend,
  DSN, data directory, native cache backend, trusted proxies, CORS origins, or
  binary upgrades.

## Edge deployments

The wasm edge entry point is invoked per platform request and does not own a
long-running Axum listener. Graceful shutdown behavior is controlled by the
edge platform, not by the native `shutdown_signal` function.
