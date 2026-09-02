---
title: "Permissions, Rate Limits & Quotas"
description: "Scoped permissions, request rate limits, and cost quotas with inheritance; how admission pre-charges and reconciles; what a rejected request sees"
---

Permissions, rate limits, and cost quotas attach to a **subject**: an
organization, a team, a user, or a single API key. A request is evaluated
against every row on its chain:

```text
api key -> user -> team -> organization
```

The console's **Access** card on any of these shows the rows set at that
level together with the rows inherited from its parents, labelled
"Inherited from ...".

## Permissions

A permission row names a subject, a provider (or all providers), an operation
group (or all operations), and an effect: allow or deny.

Evaluation runs per target provider in the resolved plan. Every row on the
chain that matches the provider and the operation group applies. Any matching
deny rejects the request; with no matching allow the request is also rejected.
There is no implicit access.

Operation groups: `models`, `count_tokens`, `memories`, `generate_content`,
`compact`, `embeddings`, `images`, `audio`, `video`, `files`, `search`,
`rerank`, `realtime`.

Permissions are by provider and operation group, not by model name. To limit
a user to some models, put those models on a provider of their own, or expose
them through a dedicated route and provider pair. The portal's **Allowed
models** list is derived from the same check: a model appears with exactly the
capabilities the user may call.

:::note
Listing models and counting tokens are `models` and `count_tokens`
operations. A user with `generate_content` only cannot list models.
:::

## Rate Limits

A rate-limit row is a subject, a request count, and a window in seconds. One
row per (subject, window length); several windows may coexist on one subject.

Windows are fixed and aligned to the Unix epoch. The counter lives in the
cache backend and is incremented before the check, so concurrent requests are
counted deterministically. A rejected request rolls its increment back and
does not consume the window. Every matching row on the chain is checked; the
first exceeded one rejects.

Per-credential requests-per-minute and tokens-per-minute limits protect an
upstream account and are configured on the credential; see
[Providers & Credentials](/guides/providers/).

## Cost Quotas

One quota row per subject holds a **total** limit and optional window limits:

| Window | Resets |
| --- | --- |
| Total | Never. |
| Daily | 00:00 UTC. |
| Weekly | Monday 00:00 UTC. |
| Monthly | First day of the month, 00:00 UTC. |
| 5-hour | Five hours after the first admitted request; the next request starts a new window. |
| 7-day | Seven days after the first admitted request, likewise. |

Limits are decimal cost in the pricing currency. A quota can be disabled
without deleting it.

### Admission

For every billable operation, admission:

1. estimates the cost as the highest price any candidate target would charge
   for the request's input tokens (counted with the tokenizer ladder);
2. adds the estimate to a pending counter for each applicable quota window
   in the cache;
3. rejects if the window is already at its limit, or if settled cost plus
   pending would exceed it;
4. records the reservation under the request id.

When the request finishes, the settled cost is written to the window (once per
request id, so retries cannot double-charge) and the pending estimate is
released. Free operations such as listing models and counting tokens skip the
quota check but still record a zero-cost settlement.

Cost comes from the price rules described in [Pricing](/reference/pricing/).
A request without a matching rule runs and records usage at zero cost.

## Rejected Requests

Every rejection is a JSON error envelope:

```json
{ "error": { "message": "quota exceeded" } }
```

| Status | Message | Cause |
| --- | --- | --- |
| `401` | `unauthorized` | Missing, unknown, disabled, or expired key. |
| `403` | `forbidden: permission denied` | No allow, or a deny, on the chain. |
| `429` | `rate limited` | A caller rate limit or a credential RPM/TPM limit. |
| `402` | `quota exceeded` | A cost window would be exceeded. |
| `404` | `unknown route or model: ...` | The model name resolves to nothing. |

## Watching Windows

**Usage** in the console lists every active quota window with a bar: settled
cost over the limit, the percentage, when the window started, and when it
resets. Bars turn to warning at 85% and critical at 100%. An anchored 5-hour
or 7-day window that has not seen a request shows "not started". The portal
shows the same bars for the signed-in user's chain, labelled by scope.

## Upstream Quota Cycles

Credentials on `codex`, `claudecode`, and `geminicli` report the upstream
account's own rate-limit windows. GPROXY records them as **quota cycles** per
credential and window key, with the boundary source (upstream, inferred, or
unknown) and used percent, and shows them on the credential card. They
describe the upstream account, not your users, and are separate from the
quotas on this page. They do steer balancing: inside a failover tier, a
credential with any live window at 90% or more sorts behind its peers, and one
at 100% sorts last. None is removed from the plan.
