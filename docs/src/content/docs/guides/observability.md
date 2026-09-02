---
title: Usage, Logs & Audit
description: "Usage rows and hourly rollups, request audit with graded capture and redaction, retention, admin action audit, request ids and process logs"
---

GPROXY writes its operational data to the persistence backend, so every
instance sharing a database shares one view. The console reads it under
**Statistics**: the Usage, Admin actions and Request audit tabs.

## Request IDs

Every gateway request gets an id of the form
`<instance>-<prefix>-<sequence>`: the numeric `GPROXY_INSTANCE_ID`
(default `0`), a random 64-bit prefix chosen at process start, and a
per-process counter, both in hex. The id is returned to the client in the
`x-request-id` response header and is the join key across usage rows,
request logs, upstream captures and the portal's recent-request list. Usage
rows also carry `instance_id` and the configured `instance_name` in their
dimensions, so a shared database can be split by instance.

## Usage Rows and Rollups

Every settled exchange writes one row to `usage_rows` and adds to the hourly
bucket in `usage_rollups` in the same batch. Settlement happens on every
path that reaches an upstream, including service surfaces; locally answered
operations record a zero-cost row.

| Column | Content |
| --- | --- |
| `request_id`, `at` | Correlation id and Unix time of settlement. |
| `provider_id`, `credential_id`, `upstream_model`, `operation` | What served the request. |
| `organization_id`, `team_id`, `user_id`, `user_key_id` | Who was admitted. |
| `input_tokens`, `output_tokens`, `cached_input_tokens` | First-class token counts; `input_tokens` includes cached reads. |
| `metrics` | Dimensional quantities such as `cache_creation_5m_tokens`, `reasoning_tokens`, `audio_seconds`, `web_searches`. |
| `dimensions` | Qualifiers such as `service_tier`, `instance_id`, `instance_name`. |
| `cost` | Settled decimal cost. |
| `usage_source` | `upstream` when the provider reported usage, `estimated` when GPROXY counted tokens itself. |
| `ended` | `complete`, or `interrupted` when the client hung up or the stream broke. |
| `latency_ms` | Wall time of the exchange. |

Rollups are keyed by hour, provider, organization, team, user, upstream
model and dimensions; the Overview trend reads them. Turning `enable_usage`
off stops persisting rows; admission, settlement and quota reconciliation
still run.

### Querying Usage

`GET /admin/api/usage?from&to` aggregates rows over a range of at most 366
days. `group_by` is `user_key`, `user`, `provider` or `model`; without it
every distinct dimension combination is returned. Filters: `user_key_id`,
`user_id`, `provider_id`, `credential_id`, `model`. Each row reports
requests, input, output and cached tokens, cache writes for 5 minutes,
30 minutes and 1 hour, and cost. The console's Usage tab exposes the same
filters with a date range and switches between **Usage and cost** and
**Quota windows**. `GET /admin/api/usage-trend?from&to` returns hourly
points.

Cost, tokens and dimensions answer different questions: cost is what was
billed after pricing; tokens are the provider's or the estimator's counts;
metrics and dimensions carry everything that is not a token and are priced
by `price_rates` rows (see [Pricing & Tiers](/reference/pricing/)).

## Request Audit

Request audit stores the downstream exchange (what the client sent and
received) and every upstream attempt it caused, correlated by request id.
Capture is off by default and graded by four switches on the Settings page
(`PATCH /admin/api/log-settings` or `/admin/api/instance-settings`).

| Switch | Records |
| --- | --- |
| `enable_downstream_log` | Client method, path, query, IP, headers, status, error kind, duration, output tokens per second. |
| `enable_downstream_log_body` | Also the client request and response bodies. Streamed responses are captured in full and written when the stream ends. |
| `enable_upstream_log` | Every upstream attempt: provider, credential, URL, method, headers, status. |
| `enable_upstream_log_body` | Also the upstream request and response bodies. |

Body switches can only be turned on when `retention_days` or
`max_database_size_mb` is set; the API answers 400 otherwise.

`GET /admin/api/logs?start&end` lists captured requests, with filters
`user_id`, `user_key_id`, `provider_id`, `status`, `request_id`, a `cursor`
and `limit` (1 to 100, default 50). `GET /admin/api/logs/<request_id>`
returns the downstream record and the ordered upstream attempts. In the
console, **Request audit** opens each request at `/admin/logs/<request_id>`
with copyable headers and bodies; fields that were not captured say so.

The client IP is the peer address, unless the peer is loopback or listed in
`GPROXY_TRUSTED_PROXIES`, in which case the first `X-Forwarded-For` entry or
`X-Real-IP` is used.

## Redaction

Redaction is on by default and applies to headers, query strings and bodies
in both directions before anything is stored:

- Headers `authorization`, `proxy-authorization`, `x-api-key`,
  `x-goog-api-key`, `api-key`, `cookie` and `set-cookie` become
  `[redacted]`.
- JSON fields and form or query parameters whose name is a known secret
  (`api_key`, `token`, `access_token`, `refresh_token`, `client_secret`,
  `password`, `code`, `signature`, `code_verifier`, `state` and similar)
  are replaced; nested objects and arrays are walked.
- Bodies over 100 MiB are truncated with a marker.

`disable_log_redaction` is the explicit cleartext override. The console
highlights it in red because credentials, cookies and user content are then
written to the database as sent. Sign-in paths (`/oauth/*`, device-auth
callbacks, `/portal/api/login`, `/portal/api/password`) stay redacted even
with the override on.

## Retention and Size Pressure

A sweep runs every five minutes.

| Setting | Effect |
| --- | --- |
| `retention_days` | Deletes `request_logs`, `wire_logs` and `usage_rows` older than the cutoff. Unset behaves as 36,500 days. |
| `max_database_size_mb` | When the database exceeds the cap, deletes the oldest 5,000 rows each of `request_logs` and `wire_logs` per sweep. Unset behaves as 1,024 MiB. |

Size pressure never deletes usage rows or rollups; only retention does, and
only by age. Database size is read from `page_count × page_size` on SQLite
and libSQL, `pg_database_size` on PostgreSQL and `information_schema`
totals on MySQL. Deleting rows does not shrink a SQLite file by itself.

## Admin Action Audit

Every successful state-changing call to the admin API writes an
`audit_events` row: actor user id, action such as `providers.update`,
`rule_preset.apply`, `credential.secret_reveal`, `user_key.reveal`,
`log_settings.update` or `channel_login.device_start`, target kind and id,
time and client IP. Sign-in events `auth.setup`, `auth.login` and
`auth.logout` are recorded too. Reads and configuration export are not
audited.

`GET /admin/api/audit?limit` returns the newest events (default 100, max
500). The **Admin actions** tab shows the latest 500 with actor name, IP,
action and target, and a text search.

## Credential Health and Quota Cycles

Health is tracked per credential and upstream model as `healthy`,
`degraded` or `dead`, with the observed status, a detail string and the
time. The Overview lists enabled credentials that are not healthy; the
credential card shows the per-model rows and offers a reset
(`POST /admin/api/credentials/<id>/health-reset`). A reset clears the
recorded state; a still-failing upstream degrades the credential again on
the next attempt.

Upstream quota windows that ride on responses or come from a quota probe are
persisted as credential cycles: window key and label, period start and end,
used and limit, whether the boundary was reported by the upstream or
inferred, and whether the cycle is open or closed.
`GET /admin/api/credential-cycles?from&to[&credential_id]` lists them; the
Usage tab and the Overview's quota-pressure card (windows at or above 80 %)
read the same data. `POST /admin/api/credentials/<id>/quota-probe`
refreshes a credential's windows on demand.

## Process Logs

The native binary logs through `tracing` to standard output.
`GPROXY_LOG_FORMAT` (`--log-format`) selects `text` (default) or
newline-delimited `json`. The level filter comes from `RUST_LOG` and
defaults to `info`. Cleanup sweeps, failed usage writes and failed captures
are logged with the request id where one exists.
