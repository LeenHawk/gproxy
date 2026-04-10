---
title: Overview
description: What GPROXY is, what it supports, and how it deploys.
---

## What GPROXY is

GPROXY is a Rust multi-channel LLM proxy. It sits between your applications and upstream LLM providers, exposing a unified API gateway that speaks OpenAI, Claude, and Gemini protocols. You configure providers and credentials once; GPROXY handles routing, protocol conversion, credential pooling, retry, failover, health tracking, and usage recording.

## Built-in channels

GPROXY ships with 14 built-in channel implementations:

| Channel | Upstream |
|---|---|
| `openai` | OpenAI API |
| `anthropic` | Anthropic Claude API |
| `aistudio` | Google AI Studio |
| `vertexexpress` | Vertex AI Express |
| `vertex` | Vertex AI (service account) |
| `geminicli` | Gemini CLI / Code Assist |
| `claudecode` | Claude Code (cookie-based) |
| `codex` | OpenAI Codex CLI |
| `antigravity` | Antigravity / Code Assist |
| `nvidia` | NVIDIA NIM |
| `deepseek` | DeepSeek |
| `groq` | Groq |
| `openrouter` | OpenRouter |
| `custom` | Any OpenAI-compatible endpoint |

The `custom` channel lets you point at any base URL with arbitrary credentials. Set `id`, `base_url`, `credentials`, and optionally override the dispatch table.

## Protocol dispatch

Every channel has a dispatch table that maps `(OperationFamily, ProtocolKind)` pairs to one of four strategies:

- **Passthrough** -- forward the request as-is. Same protocol in, same protocol out. Minimal parsing overhead.
- **TransformTo** -- convert the request to a different `(operation, protocol)` before sending upstream. For example, an OpenAI Chat Completions request hitting an Anthropic provider gets transformed to Claude Messages format.
- **Local** -- handle the request without contacting upstream (e.g. model list from a static table).
- **Unsupported** -- return 501.

The six protocol kinds GPROXY understands:

| Protocol | Wire format |
|---|---|
| `openai` | OpenAI Chat Completions (legacy-style) |
| `openai_chat_completions` | OpenAI Chat Completions |
| `openai_response` | OpenAI Responses API |
| `claude` | Anthropic Messages |
| `gemini` | Gemini GenerateContent |
| `gemini_ndjson` | Gemini streaming (NDJSON) |

Cross-protocol conversion happens automatically. You can send an OpenAI-format request to a Claude provider, or a Claude-format request to a Gemini provider. GPROXY transforms both the request and the response.

## Operation families

Routing dispatches by operation, not raw URL path. The operation families are:

`ModelList`, `ModelGet`, `GenerateContent`, `StreamGenerateContent`, `CountToken`, `Compact`, `Embedding`, `CreateImage`, `StreamCreateImage`, `CreateImageEdit`, `StreamCreateImageEdit`, `OpenAiResponseWebSocket`, `GeminiLive`, `FileUpload`, `FileList`, `FileGet`, `FileContent`, `FileDelete`

## Multi-database support

The storage layer uses SeaORM with three database drivers compiled in:

- **SQLite** -- default, zero-config. GPROXY creates the file on first run.
- **MySQL** -- set `--dsn mysql://user:pass@host/db`
- **PostgreSQL** -- set `--dsn postgres://user:pass@host/db`

Schema sync runs automatically at startup. Switching databases requires only changing the DSN.

## Single-binary deployment

The React admin console (built with Vite + Tailwind) is compiled into the binary via `rust-embed`. One process serves everything:

- Console UI at `/console/*`
- API routes at `/v1/*`, `/admin/*`, `/user/*`
- Provider-scoped routes at `/<provider>/v1/*`

Root `/` redirects to `/console/login`.

No Nginx, no Node.js runtime, no separate frontend deploy. Ship one binary and optionally a `gproxy.toml`.

## Workspace layout

```
gproxy/
  apps/gproxy/          -- binary entry point, Axum server, embedded console
  sdk/gproxy-protocol/  -- OpenAI/Claude/Gemini types and cross-protocol transforms
  sdk/gproxy-provider/  -- Channel trait, dispatch tables, credential retry, billing
  sdk/gproxy-routing/   -- Route classification, model aliases, permissions, rate limits
  sdk/gproxy-sdk/       -- top-level SDK re-exports
  crates/gproxy-core/   -- domain services (config, identity, policy, quota, routing, file)
  crates/gproxy-server/ -- AppState, middleware, session management
  crates/gproxy-api/    -- HTTP handlers (admin, user, provider), bootstrap, login
  crates/gproxy-storage/-- SeaORM entities, repositories, async write sink
  frontend/console/     -- React admin console source (@gproxy/console)
```
