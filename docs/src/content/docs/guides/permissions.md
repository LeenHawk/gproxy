---
title: "Permissions, rate limits, and budgets"
description: "Configure access rules, request limits, and budgets, and distinguish local limits from upstream allowance."
---

Permissions control access, rate limits control request frequency, and cost budgets control spending. These checks are separate.

## Permissions

A permission rule belongs to a user or API key and may filter by provider, model pattern, and operation. Model patterns support `*` and `?`, so model restrictions do not require separate providers.

Rules are ordered by descending priority, then ID. The first matching rule supplies the `allow` or `deny` decision. Ordinary users need an applicable allow rule. Instance administrators bypass ordinary permission rules, while OAuth access tokens are still limited to a fixed operation baseline (model catalogue, token counting, generation, streaming, compaction) unless the client is listed in `oauth.cli_client_ids`. The `scope` a client requests is recorded on the grant but does not widen or narrow this baseline.

| API field | Meaning |
| --- | --- |
| `userId` / `apiKeyId` | Exactly one rule subject |
| `providerId` | Optional; omit to match all providers |
| `modelPattern` | Model pattern, default `*` |
| `operation` | Optional; omit to match all operations |
| `action` | `allow` or `deny` |
| `priority` | Higher values match first |

The model catalog's `permitted` field reports caller access. An inaccessible model may still appear in the catalog.

## Credential visibility

Permission alone is not enough: the caller also needs an available credential. Credential ownership and the API key's user, organization, and team bindings determine visibility. Management scope, model permission, and credential ownership are separate settings.

Vendor service routes (account, usage, and other non-model endpoints a channel declares) have no model or operation, so only a rule covering the whole provider — any model, no operation — allows or denies them. They are outside the OAuth operation baseline.

## Rate limits

User or API-key rate limits specify a metric, limit, period in seconds, optional model pattern, and enabled state. Common metrics are `requests` and `concurrency`. Live counters use the cache backend; multiple instances need shared cache state.

Permission checks run before rate-limit accounting. Inspect the error and logs to distinguish caller limits from upstream credential limits.

## Cost budgets

Budgets can belong to users, API keys, organizations, and teams, in USD. Requests check the key and user budgets plus those of the key's actual organization and team bindings. A team's parent organization is not charged merely because that relationship exists.

Every applicable enabled budget must have remaining capacity. Period and model filters are configurable in the console. Actual cost is settled after the upstream call, so in-flight and concurrent calls may exceed the limit. A budget is not a hard cap on upstream spending.

Missing prices affect cost totals and budget consumption. Check [Pricing](/reference/pricing/) as well. Budget amounts use decimal strings in the API.

## Provider and credential limits

A provider's default limits apply separately to each credential, not to one shared provider pool. An enabled same-name credential rule overrides the provider default. Restoring inheritance removes the individual configuration.

Local limits differ from upstream account allowance. Resetting a local rule does not reset the upstream balance or window. See [Providers and credentials](/guides/providers/) and [Console](/guides/console/).

## Diagnose a rejection

- `401`: check key and user validity.
- `403`: check permissions, credential visibility, and the OAuth operation baseline.
- `429`: check caller rate limits, budgets, local credential limits, and upstream allowance.

Inspect the relevant user, key, and credential in the console, then use the specific logged error to identify the limiting rule.
