---
title: Adding a Channel
description: Implement a new v2 channel adapter, declare its Operation routing surface, and register it in the built-in registry.
---

A channel is the upstream access adapter. It injects authentication, resolves the
endpoint URL, classifies upstream responses, and optionally shapes provider
quirks. It does not own cross-protocol transforms or provider rule-set
processing.

```text
transform/     protocol conversion by Operation
process/       provider rule sets after transform
channel/       upstream access, auth, endpoint, response disposition
```

Keep new channel work within that boundary.

## Start from a Similar Channel

Channels live under `src/channel/bulletins/`. Pick the closest existing adapter:

| Upstream shape | Starting point |
| --- | --- |
| OpenAI-compatible API key | `openai`, `custom`, `deepseek`, `groq`, `nvidia` |
| Anthropic Messages | `claudeapi` |
| Gemini API key | `aistudio`, `vertexexpress` |
| Vertex service account | `vertex` |
| OAuth or agent envelope | `codex`, `claudecode`, `geminicli`, `antigravity`, `kiro`, `copilotcli` |

Most channel folders split auth, routes, shaping, OAuth, or stream decoding into
small files. Follow that local shape instead of growing one large module.

## Implement `Channel`

The required methods are:

| Method | Responsibility |
| --- | --- |
| `id()` | Stable registry id; must match `Provider.channel`. |
| `provider_family()` | `open_ai`, `claude`, or `gemini` family for usage/billing context. |
| `routing_table()` | Declared `(Operation, OperationKind) -> RoutingDecision` surface. |
| `prepare()` | Build the absolute upstream request and inject auth. |

Important optional hooks:

| Hook | Use when |
| --- | --- |
| `classify()` | Upstream status/body needs provider-specific retry, cooldown, or auth-dead handling. |
| `shape_request()` | Provider-native body needs hygiene after transform/process rules. |
| `shape_response()` | Raw upstream body needs normalization before response transform. |
| `stream_decoder()` | The upstream stream is enveloped or binary and must be unwrapped before SSE transform. |
| `needs_refresh()` / `refresh()` | Credentials are OAuth-like and must be refreshed before use. |
| `prepare_usage_request()` / `parse_usage()` | The provider exposes a per-credential usage/quota endpoint. |
| `default_emulation()` | Native `wreq` should use a built-in TLS/HTTP2 impersonation profile. |

`prepare()` receives the effective body after protocol transform and rule-set
processing. Do not mutate the body there. Use `shape_request()` for
channel-local field hygiene.

## Declare Operation Routing

Use the helpers in `src/channel/routes.rs`:

```rust
use crate::channel::routes::{cg, pass, pv, xform};
use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

vec![
    pass(ListModels, pv(P::OpenAi)),
    xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
    pass(GenerateContent, cg(OpenAiChatCompletions)),
    xform(GenerateContent, cg(ClaudeMessages), GenerateContent, cg(OpenAiChatCompletions)),
]
```

Routing is operation-first. Do not create "OpenAI bucket" or "Claude bucket"
logic. Each cell says whether this channel can serve that operation/kind by
passthrough, transform, local handling, or unsupported.

Provider creation materializes this route list into stored `routing_rules`.
Runtime dispatch reads the stored rules; a missing row is unsupported.

## Register the Channel

Add the module under `src/channel/bulletins/mod.rs`, then add the channel to
`builtin_channels()` in `src/channel/registry.rs`. If the channel supports
interactive login, also add a `ChannelLogin` implementation to
`builtin_logins()`.

## Add Console Metadata

The console needs enough metadata to create providers and credentials for the
new channel. Check `console/src/lib/channel-meta.ts` and the provider /
credential forms. Prefer presets and UI helpers for provider-specific policy;
only add runtime primitives when the backend truly cannot express the behavior.
