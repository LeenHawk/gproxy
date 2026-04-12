---
title: Providers & Channels
description: How providers, channels, and credentials fit together in gproxy.
---

A **provider** in GPROXY is a named upstream LLM endpoint. Each provider is
backed by exactly one **channel** (the code that speaks the upstream's
protocol) and has one or more **credentials** attached to it.

```text
Provider  ──(channel)──►  upstream protocol implementation
   │
   ├── settings     (base_url, timeouts, …)
   └── credentials  (api keys, OAuth, service account, …)
```

## Built-in channels

The `gproxy-provider` crate ships the following channels. Feature flags of the
same name (see the [Rust SDK](/reference/sdk/) reference) let you compile only
the subset you need.

| Channel | Typical upstream | Notes |
| --- | --- | --- |
| `openai` | api.openai.com, any OpenAI-compatible gateway | Use `custom` to override base URL freely. |
| `anthropic` | api.anthropic.com | Claude Messages API. |
| `aistudio` | Google AI Studio | `generativelanguage.googleapis.com`. |
| `vertex` | Google Vertex AI | Uses a GCP service account. |
| `vertexexpress` | Vertex AI Express mode | Simpler Gemini access via an API key. |
| `geminicli` | Local Gemini CLI tooling | For local dev / bridging. |
| `claudecode` | Anthropic Claude Code | Developer tooling channel. |
| `codex` | Codex-family endpoints | |
| `antigravity` | Antigravity agent runtime | |
| `deepseek` | api.deepseek.com | OpenAI-compatible DeepSeek. |
| `groq` | api.groq.com | |
| `openrouter` | openrouter.ai | |
| `nvidia` | NVIDIA NIM endpoints | |
| `custom` | Any OpenAI-compatible upstream | Use this for self-hosted or third-party gateways. |

## Defining a provider

In the seed TOML:

```toml
[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [
  { api_key = "sk-upstream-1" },
  { api_key = "sk-upstream-2" }
]
```

Or, at runtime, use the *Providers* tab in the embedded console. `settings`
and each `credentials[i]` are free-form JSON blobs — their exact shape
depends on the channel (`OpenAiSettings`, `AnthropicCredential`, and so on).
The console renders a structured editor for each one.

## Credentials and health

When a provider has multiple credentials, GPROXY treats them as a rotation
pool. The `GproxyEngine` picks a credential, calls upstream, and on failure
updates a **per-credential health state** (cooldowns for rate-limited keys,
disables for revoked keys, and so on). The `HealthBroadcaster` worker
debounces those changes before writing them to storage so a burst of
failures doesn't spam the database.

You can watch the current health state from the console or from the
`/admin/health` endpoints.

## Two routing modes

Every provider is reachable two ways on the same GPROXY instance:

| Mode | URL shape | How the provider is chosen |
| --- | --- | --- |
| **Aggregated** | `/v1/...`, `/v1beta/...` | From a `provider/model` prefix in the `model` field (or from an alias). |
| **Scoped** | `/{provider}/v1/...` | From the URL path; the `model` field carries just the upstream id. |

Both modes serve every protocol (OpenAI / Claude / Gemini) and both go
through the same `permission → rewrite → alias → execute` pipeline. Pick
whichever fits your client — see
[First Request](/getting-started/first-request/) for concrete curl
examples of each.

## Same-protocol passthrough

If the client and the selected upstream speak the same protocol (for
example, an OpenAI-compatible client hitting an OpenAI provider), GPROXY
forwards the request with **minimal parsing**. It still applies auth, model
resolution, usage accounting, and rate limiting — it just avoids
deserializing the body when nothing needs to be rewritten. This is the hot
path and where GPROXY gets most of its throughput.

When the protocols differ, the `gproxy-protocol::transform` layer converts
the request shape on the way in and the response shape on the way out.

## See also

- [Models & Aliases](/guides/models/) — how resolved `(provider, model)`
  pairs are named and routed.
- [TOML Config reference](/reference/toml-config/) — every field supported
  by the seed file.
