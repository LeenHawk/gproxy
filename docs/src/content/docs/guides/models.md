---
title: Models, Routes, and Aliases
description: Explain how v2 resolves client model names through aliases, routes, provider models, route members, variants, and pricing.
---

In v2, a client model name is not necessarily an upstream model id. Aggregated
traffic resolves the request `model` through aliases and routes before a
provider credential is selected.

```text
request model
  -> global alias
  -> route or provider/model
  -> provider alias
  -> provider + upstream_model_id
  -> provider credential
```

Scoped provider traffic skips the route lookup because the provider comes from
the URL, but it still uses the provider model catalogue for model listing,
variant stripping, and visibility.

## Provider Models

`provider_models` is the local catalogue for a provider. Each row has:

| Field | Meaning |
| --- | --- |
| `provider_id` | Owning provider. |
| `model_id` | Upstream model id. |
| `display_name` | Optional friendly name. |
| `variants_json` | Optional suffix-variant exposure config. |
| `enabled` | Disabled models are not exposed. |

The console can pull a live upstream model list with
`/admin/providers/{provider_id}/upstream-models`. Pulling models is an admin
operation; it calls the provider or returns bundled models if the channel ships
a static catalogue.

## Routes and Members

A route is the exposed model name for aggregated mode. A route can have one or
more members:

| Record | Key fields |
| --- | --- |
| `routes` | `name`, `strategy`, `enabled`, optional `settings_json`. |
| `route_members` | `provider_id`, `upstream_model_id`, `tier`, `weight`, `enabled`. |
| `aliases` | `provider`, regex `alias`, replacement `target`, `sort_order`, `enabled`. |

Members are pre-sorted by `tier` ascending and `weight` descending in the
snapshot. The balance layer then applies the route strategy and provider
credential strategy.

Aliases are ordered full-match regex replacements. `provider="*"` applies
before route/provider resolution; provider-scoped aliases apply after the
provider is known. Permissions are checked against the exposed route or provider
name, not hidden credential material.

## Model Listing

Model-list endpoints are classified as the `Models` operation group. The inbound
wire kind is inferred from the endpoint and credential style:

- OpenAI and Claude share `/v1/models`; Claude callers are detected by
  `x-api-key`, OpenAI callers by `Authorization`.
- Gemini uses `/v1beta/models`.
- `GET /v1/models/{id}` and `GET /v1beta/models/{id}` classify as `get_model`.

Routing rules decide whether a provider handles the operation as `local`,
`passthrough`, `transform_to`, or `unsupported`. Local model listing is served
from the snapshot and filtered by the authenticated user's permissions.

## Variants

`variants_json` lets one provider model expose suffix variants. The snapshot
build compiles enabled provider models into:

- an exposed model list for model-list responses;
- a variant-to-base map so request-side suffixes can be stripped before the
  upstream call.

Use variants for provider-supported model suffixes that should be visible to
clients without duplicating a full model row for every exposed id.

## Pricing

Pricing is stored in dedicated `price_rules`, not on provider model rows. A
rule can be provider-scoped or global, and uses `match_type` plus `model_match`
to match the upstream model id.

The resolver checks:

1. provider exact;
2. global exact;
3. provider contains;
4. global contains.

Price fields include `input_price`, `output_price`, `cache_read_price`,
`cache_creation_5m_price`, `cache_creation_30m_price`,
`cache_creation_1h_price`, and `image_price`. Token
prices are per million tokens. Image price is a flat per-image value. Missing
rules default to zero: usage is still recorded, but the call bills nothing.
