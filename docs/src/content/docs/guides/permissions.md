---
title: Permissions, Rate Limits, and Quotas
description: Configure route access, request limits, token limits, and spend quotas across org, team, and user scopes.
---

Authorization in v2 is scope-based. Permissions, rate limits, and quotas can be
attached to an org, a team, or a user.

```text
user scope -> team scope -> org scope
```

The snapshot stores all matching rows by `(scope, scope_id)`, and the hot path
checks them without reading persistence.

## Route Permissions

A route permission grants access to route or provider names by glob pattern:

```json
{
  "scope": "team",
  "scope_id": 42,
  "route_pattern": "chat-*"
}
```

Effective permission is the union of user, team, and org patterns. If any
pattern in the chain matches the exposed name, the request can proceed. If no
pattern matches, the request is denied. A disabled org or disabled team denies
even if a lower scope has a matching pattern.

Permissions match the exposed route name in aggregated mode and the exposed
provider name in scoped mode. They do not match hidden route members,
credentials, or internal upstream model ids.

## Rate Limits

Rate-limit rows are also scoped and route-patterned:

| Field | Meaning |
| --- | --- |
| `rpm` | Requests per minute. |
| `rpd` | Requests per day. |
| `total_tokens` | Daily token budget checked from settled token counters. |

Rows are checked from most specific to least specific: user, then team, then
org. The first exceeded matching rule wins.

Request counters use the cache backend and are incremented before the over-limit
check. That makes concurrent enforcement deterministic, at the cost of rejected
requests consuming request-count budget. If the counter backend is unavailable,
v2 fails closed for enforced limits.

## Quotas

A quota is a spend ceiling for one scope:

```json
{
  "scope": "org",
  "scope_id": 1,
  "quota_total": "100.00",
  "cost_used": "12.50"
}
```

Every quota on the user's chain must fit. Admission considers both persisted
`cost_used` and in-flight pending spend. After the request settles, actual usage
reconciles pending quota and updates persisted cost.

Pricing comes from `provider_models.pricing_json`. Unpriced models still run and
record usage, but add zero cost.

## Order in the Request Lifecycle

The relevant lifecycle order is:

```text
auth user API key
  -> preprocess route/provider name
  -> permission and rate-limit admission
  -> estimate quota and pre-deduct pending spend
  -> balance, transform, process, channel
  -> settle actual usage and quota
```

This order means authorization sees the exposed route/provider name before the
request is transformed to provider-native format.
