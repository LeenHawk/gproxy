---
title: Permissions, Rate Limits & Quotas
description: How to gate access to models, throttle traffic, and enforce cost ceilings per user.
---

Three independent mechanisms control what a user can do:

1. **Permissions** — *can this user call this model at all?*
2. **Rate limits** — *how fast can they call it?*
3. **Quotas** — *how much total cost can they accrue?*

All three are matched per request before the upstream call. Each one can
deny on its own, returning a specific error code.

## Permissions

A permission row is a `(user, provider, model_pattern)` tuple:

```toml
[[permissions]]
user_name = "alice"
provider_name = "openai-main"
model_pattern = "gpt-*"
```

- `model_pattern` is a **glob** (`*`, `?`) against the incoming model name
  **before** alias resolution. A permission on `chat-*` matches the alias
  `chat-default`; a permission on `gpt-*` matches the raw id `gpt-4.1-mini`.
- `provider_name` can be omitted to grant access across all providers (the
  match still goes through permission → rewrite → alias → execute).
- A user with no matching permission row gets `403 forbidden: model`.

### File permissions

`[[file_permissions]]` is a separate table that grants a user the ability
to call file-related upstream endpoints (upload, retrieve, delete) on a
provider. The structure is simpler:

```toml
[[file_permissions]]
user_name = "alice"
provider_name = "openai-main"
```

## Rate limits

Rate limits are scoped to `(user, model_pattern)` and can enforce any or
all of three counters:

| Field | Meaning |
| --- | --- |
| `rpm` | Requests per **minute**. |
| `rpd` | Requests per **day**. |
| `total_tokens` | Total **tokens** in a rolling window. |

```toml
[[rate_limits]]
user_name = "alice"
model_pattern = "gpt-*"
rpm = 60
rpd = 10000
total_tokens = 200000
```

Counters are maintained in memory and reconciled through the
`RateLimitGC` worker, which garbage-collects expired windows so the
process doesn't accumulate dead counters.

A request that trips any counter returns `429 rate_limited` with headers
indicating which counter fired and when the window resets.

## Quotas

A quota is a single USD-denominated ceiling per user:

```toml
[[quotas]]
user_name = "alice"
quota = 100.0       # Ceiling in USD
cost_used = 0.0     # Consumed cost; increased by usage accounting
```

- The `UsageSink` worker writes cost into `cost_used` as requests
  complete.
- The `QuotaReconciler` worker periodically recomputes `cost_used` from
  recorded usage rows, so the live counter self-corrects if anything
  drifts.
- A request whose projected cost would push `cost_used` over `quota`
  returns `402 quota_exceeded`.

Quota is a *billing* ceiling, not a safety ceiling — it only exists for
models and aliases that have pricing configured. Models without prices
contribute $0 to `cost_used`.

## Order of evaluation

For a given request, the checks run in this order:

1. **Auth** — resolve API key to user.
2. **Permission** — is there a matching `[[permissions]]` row? If not →
   `403 forbidden: model`.
3. **Quota** — is `cost_used < quota`? If not → `402 quota_exceeded`.
4. **Rate limit** — is the matching counter within its ceiling? If not
   → `429 rate_limited`.
5. **Execute** — route through `rewrite → alias → execute`.

So a permission denial always beats a rate-limit denial, which always
beats execution errors from upstream.

## Runtime management

Everything described here can be edited at runtime from the console's
*Users* → *Permissions / Rate limits / Quota* workspace, or via the admin
API under `/admin/permissions`, `/admin/rate_limits`, `/admin/quotas`.
Changes are applied on the next request; there is no need to reload.
