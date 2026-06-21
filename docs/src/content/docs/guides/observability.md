---
title: Observability
description: Understand v2 usage rows, request logs, audit logs, credential health, settings, retention, and Prometheus metrics.
---

v2 writes operational data to the persistence backend so native, multi-instance,
and edge deployments can share the same view. Hot-path settings are loaded into
the control-plane snapshot.

## Request IDs

Every gateway request gets a request id. Usage records, downstream logs, and
upstream logs carry that id so one call can be joined across tables and console
views.

## Usage

Usage records include:

- request id and timestamp;
- route name, provider id, credential id;
- org, team, user, and user key ids;
- operation and kind;
- model;
- input, output, cache-read, and cache-creation tokens;
- cost;
- latency and usage source.

Usage is controlled by `instance_settings.enable_usage`, which defaults to true.
Settlement also updates quotas and token-limit counters.

## Request Logs

Request logging is split into downstream and upstream streams:

| Setting | Captures |
| --- | --- |
| `enable_downstream_log` | Client-facing method, path, query, status, headers. |
| `enable_downstream_log_body` | Downstream request and response bodies. |
| `enable_upstream_log` | Provider URL, method, status, latency, headers. |
| `enable_upstream_log_body` | Upstream request and response bodies. |

Redaction is on by default. `disable_log_redaction` exists for debugging, but it
can expose secrets and should not be enabled casually.

## Audit Logs

Admin and portal mutation paths emit audit rows with actor id/name, action,
target, status, and source IP. Use these to answer "who changed the control
plane" rather than to debug LLM payloads.

## Credential Health

Credential status rows track each credential/channel pair:

- `health_kind`;
- optional structured `health_json`;
- `checked_at`;
- `last_error`.

The pipeline and channel response classifier decide when a credential should be
retried, cooled down, or treated as auth-dead. The console shows current status
through `/admin/credential-statuses`.

## Metrics

`/metrics` is admin-gated and renders Prometheus text from persisted aggregate
data, not process-local counters. Current families include:

- `gproxy_requests_total`
- `gproxy_tokens_total`
- `gproxy_upstream_latency_ms`
- `gproxy_credential_health`
- `gproxy_quota_total`
- `gproxy_quota_used`

This design keeps metrics meaningful across native multi-instance and edge
deployments where process-local counters would be misleading.

## Retention

`instance_settings.retention_days` controls cleanup of usage and request-log
rows. `None` or a non-positive value retains rows indefinitely. Retention is for
logs and usage data; it should not delete business/control-plane records.
