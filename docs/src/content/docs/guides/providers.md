---
title: Providers and Channels
description: Configure upstream providers, credentials, routing capabilities, proxies, TLS profiles, and scoped provider access in GPROXY v2.
---

A **provider** is one named upstream endpoint. It binds a stable name to a
channel adapter, provider settings, credential pool, model catalogue, routing
rules, and optional process rule sets.

```text
Provider
|-- channel                 openai, claudeapi, aistudio, codex, ...
|-- settings_json           base_url and channel-specific knobs
|-- credentials             sealed API keys, OAuth tokens, service accounts
|-- provider_models         local model catalogue and pricing
|-- routing_rules           Operation + OperationKind dispatch table
`-- provider_rule_sets      attached request mutation rule sets
```

The hot path reads providers from `ControlPlaneSnapshot`. Admin changes are
written to persistence, then the snapshot is rebuilt and invalidated so the next
request sees the new control plane without restarting the process.

## Built-in Channels

The native and edge runtimes build the same channel registry. Current built-in
channel ids are:

| Channel id | Typical use |
| --- | --- |
| `openai`, `custom` | OpenAI API or OpenAI-compatible gateways. |
| `openrouter`, `deepseek`, `groq`, `nvidia`, `vercel` | API-key providers with OpenAI-like surfaces. |
| `claudeapi` | Anthropic Claude Messages API. |
| `aistudio`, `vertex`, `vertexexpress` | Gemini / Vertex upstreams. |
| `codex`, `claudecode`, `geminicli`, `antigravity`, `kiro`, `copilotcli` | OAuth, device-code, cookie, or envelope-style agent channels. |

Every channel declares a routing surface as `(Operation, OperationKind) ->
RoutingDecision`. That is the source for the provider's default
`routing_rules` rows. Request behavior is therefore described by operation
capability, not by provider-family buckets.

## Provider Fields

The provider record carries:

| Field | Meaning |
| --- | --- |
| `name` | Unique provider name. Scoped routes use this in the URL. |
| `channel` | Channel registry id, such as `openai` or `claudeapi`. |
| `settings_json` | Free-form channel settings. Common keys include `base_url` and channel toggles. |
| `credential_strategy` | Credential-pool strategy, currently `round_robin` or `sticky`. |
| `proxy_url` | Native outbound proxy fallback for the provider. Edge ignores native proxy settings. |
| `tls_fingerprint` | Optional provider-level TLS/HTTP2 emulation profile. Credential settings can override it. |
| `enabled` | Disabled providers disappear from routing. |

Credential rows belong to a provider. They carry `kind`, sealed `secret_json`,
`weight`, optional `rpm_limit` / `tpm_limit`, optional proxy and TLS overrides,
and an `enabled` flag. Secrets are redacted in debug output and sealed when a
master key is configured.

## Aggregated and Scoped Access

GPROXY v2 supports two ways to reach an upstream:

| Mode | URL shape | Resolution |
| --- | --- | --- |
| Aggregated | `/v1/*`, `/v1beta/*` | The request `model` resolves through alias / route tables, then to a route member and credential. |
| Scoped | `/{provider}/v1/*`, `/{provider}/v1beta/*` | The provider name comes from the path; the model goes directly to that provider. |

Both modes use the same classifier, auth, transform, process, channel, and
settle layers after resolution. Aggregated mode is the normal multi-provider
gateway path. Scoped mode is useful for debugging or exposing one provider
without creating a route.

## Routing Rules

Routing rules are provider-local. Each row names:

- `operation`: for example `generate_content`, `stream_generate_content`,
  `count_tokens`, or `create_embedding`.
- `kind`: one of the content-generation wire kinds
  `open_ai_responses`, `open_ai_chat_completions`, `claude_messages`,
  `gemini_generate_content`, or provider kinds `open_ai`, `claude`, `gemini`.
- `implementation`: `passthrough`, `transform_to`, `local`, or `unsupported`.
- optional `dest_operation` and `dest_kind` for `transform_to`.

No matching routing rule means `unsupported`. Defaults are materialized into
stored rows when a provider is created, and the console can reset them from the
channel defaults.

## Provider Rule Sets

Reusable rule sets attach to providers through `provider_rule_sets`. Attached
sets are flattened in attachment order, compiled once during snapshot rebuild,
then applied after protocol transform and before channel preparation. This is
where system-text injection, cache breakpoints, field rewrites, sanitization,
and header rules currently run.

The current backend is intentionally permissive: invalid rules warn and skip,
and provider-specific policy should live in console/config presets unless the
runtime needs a genuinely new primitive.
