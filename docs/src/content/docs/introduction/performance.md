---
title: Performance
description: What GPROXY costs per request, how much it can settle per second, how many connections it holds, and how those numbers were measured.
---

GPROXY is infrastructure. It sits between every client and every provider,
so it is designed to be the part of the path you never have to think about.
This page shows what that means in numbers, and how the numbers were
obtained, so you can judge them against your own deployment.

## Headline Numbers

Every figure below is end to end: the request is authenticated against a
user key, routed through a model alias to a provider credential, sent
upstream, priced, and written to the usage ledger. Nothing was switched off
to make the gateway look faster.

| Measure | Result |
|---|---|
| Latency added to one request, single connection | 0.21 ms median, 0.50 ms p99 |
| Metered throughput, 32 connections | 27,000 requests/s, p50 1.0 ms, p99 4.4 ms |
| Metered throughput, 256 connections | 23,700 requests/s, p99 26 ms |
| Metered throughput, 2,000 connections | 22,900 requests/s |
| Authentication and routing only, no upstream | 108,000 requests/s |
| 10,000 concurrent connections, 500 ms upstream | 18,600 requests/s, +1.4 ms median over the upstream |
| 10,000 concurrent streams, about 1.5 s each | 6,300 streams/s, +15 ms median over the upstream |
| 20,000 concurrent connections, 500 ms upstream | 18,500 requests/s, zero errors |
| Resident memory with 10,000 connections open | about 1.2 GB |
| Resident memory under sustained load, no backlog | about 230 MB |

## How It Was Measured

- **Machine.** One laptop-class host: AMD Ryzen 7 8745H, 16 threads, 28 GB
  RAM, NVMe storage. The load generator, the gateway and the mock upstream
  all ran on it and shared those cores. A gateway on its own host has more
  headroom than these numbers show.
- **Storage.** The default SQLite backend on disk, write-ahead log,
  `synchronous=NORMAL`. No tmpfs, no in-memory database.
- **Upstream.** A local mock that speaks the OpenAI Chat Completions wire
  format. For the latency-focused runs it answered immediately; for the
  concurrency runs it held each reply for 500 ms, or streamed twenty chunks
  50 ms apart, to behave like a real model.
- **Load.** `oha` over HTTP/1.1 keep-alive, one request in flight per
  connection, runs of 10 to 25 seconds. Every run was checked afterwards:
  the number of usage rows in the database matched the number of requests
  the upstream received.
- **Controls.** Each concurrency scenario was repeated directly against the
  mock, bypassing the gateway, so the gateway's own contribution could be
  separated from the load generator's and the kernel's.

## Why It Is Fast

- **Nothing on the request path touches the database.** The control plane
  is an in-memory snapshot swapped atomically on change. Keys, routes,
  credentials, prices and health all resolve from memory, including the
  decrypted credential itself.
- **Zero-copy data plane.** Bodies move as reference-counted bytes. A stream
  passes through untouched unless a protocol transform has to rewrite it, and
  a body is parsed once, at the stage that needs it.
- **Settlement runs after the reply.** The usage row, rollups and health
  observations are written once the response has left. The backlog is
  bounded, so a burst cannot grow memory without limit; when it fills, the
  response path waits instead.
- **Grouped commits.** The SQLite thread folds every settlement already
  queued into one transaction. Under load a burst pays one write-ahead-log
  commit instead of one per request, which is what moved the ceiling from a
  few thousand to tens of thousands of settlements per second.
- **Pairwise transforms, no intermediate format.** Converting between
  OpenAI, Claude and Gemini shapes is a direct rewrite, not two hops through
  a pivot representation.

## What This Means for You

With a real model behind the gateway, the time a request spends inside
GPROXY is a rounding error. A typical completion takes hundreds of
milliseconds to tens of seconds upstream; the gateway adds a fraction of a
millisecond at the median and a few milliseconds at the tail.

Throughput is rarely the limit either. A single instance settles more
requests per second than most deployments send in a minute, and the
bottleneck at that point is the SQLite ledger, which can be replaced with
PostgreSQL or MySQL for multi-instance deployments.

Concurrency is what you tune. The native host caps in-flight requests with
`GPROXY_MAX_IN_FLIGHT`, which defaults to 1024. Raise it to the number of
simultaneous requests you expect, and make sure the process may open enough
file descriptors: every in-flight request holds one connection from the
client and one to the provider. Budget roughly 110 KB of resident memory
per open connection.

## Reproducing

The harness is a mock upstream, a seed script that creates a provider, a
route, a user and a key through the admin API, and an `oha` runner. Any
HTTP load tool works: point it at `/v1/chat/completions` with a bearer key,
and compare the count of usage rows with the count of requests afterwards.
If the two differ, that is a bug worth reporting.
