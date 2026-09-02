---
title: What is GPROXY?
description: What GPROXY does, who it is for, how a request moves through it, and the concepts you will meet in the console.
---

**GPROXY** is a self-hosted gateway for LLM APIs. Your clients call one base
URL with one API key. GPROXY authenticates the key, picks an upstream provider
and credential, converts the request when the client and the upstream speak
different wire formats, applies your rules and spending limits, and records
what every request cost. One binary serves the gateway, a public site at `/`,
the operator console at `/admin`, and a user portal at `/portal`.

## Who It Is For

- Operators who hold several upstream accounts and want one pool behind a
  single endpoint, with failover and per-credential health tracking.
- Teams that want application code to depend on one model name rather than
  on one vendor's SDK.
- Users of Codex CLI or Claude Code who want those tools to run against
  pooled credentials with every request metered.
- Anyone who needs per-user permissions, rate limits, and cost quotas in
  front of LLM traffic.

## Accepted Wire Formats

GPROXY accepts OpenAI Chat Completions, OpenAI Responses, Claude Messages, and
Gemini GenerateContent. A request in any of these formats can be served by an
upstream that speaks any other: all six pairwise conversions exist, for
buffered and streaming responses. Conversion is direct between the two
formats; there is no intermediate schema. Streams are delivered as SSE, as a
WebSocket for OpenAI Responses, or as Gemini's incremental JSON array
(`?alt=sse` selects SSE on Gemini paths).

Other operation groups take the same path: embeddings, images, audio, video,
rerank, files, web search, token counting, compaction, model listing,
realtime, memories, and guardian.

## How a Request Is Handled

1. **Ingress and classification.** The method and path select the operation.
   The first path segment may name a target (see below). The `stream` flag or
   the path decides whether the response streams.
2. **Authentication.** The API key is read from `Authorization: Bearer`,
   `x-api-key`, or `x-goog-api-key` and resolves to a user, team, and
   organization.
3. **Routing.** The model name is preprocessed in a fixed order — alias, then
   variant suffix, then route — and becomes an ordered list of candidates:
   route members by tier and weight, then a credential from the provider's
   pool. Selection is a deterministic rotation, never random.
4. **Admission.** Permissions, rate limits, and cost quotas are checked at
   every scope the key inherits from. Quotas pre-charge an estimate.
5. **Transform.** If the upstream speaks another format, the request is
   converted. The provider's rule sets then run on the provider-native
   request.
6. **Upstream.** The channel authenticates and sends the request. A failed
   attempt moves to the next candidate, up to the route's or the instance's
   attempt limit (default 6).
7. **Settle, capture, telemetry.** The response is converted back, usage is
   extracted and priced, the quota estimate is reconciled to the settled
   cost, and the exchange is recorded according to the log settings.

Every path that reaches an upstream — SDK calls, CLI control planes,
WebSockets — passes through the same stages. There is no unmetered shortcut.

## Two Ways to Address the Gateway

| Mode | Path shape | What selects the upstream |
| --- | --- | --- |
| Aggregated | `/v1/chat/completions`, `/v1/responses`, `/v1/messages`, `/v1beta/models/{model}:generateContent` | The model name, resolved through aliases, variants, and routes. |
| Named | `/{target}/` followed by the native path, for example `/codex/v1/responses` or `/codex/backend-api/codex/responses` | The first segment: a provider name, a route name, or the namespace prefix of an exposed model such as `openai/...`. |

With a provider name, the `model` field is the upstream model id and the
provider's credential pool is used directly. With a route name, the route's
members are used. With a namespace, `model` is the part after the slash. A
first segment that matches nothing is treated as an aggregated path.

## Core Concepts

| Concept | What it means |
| --- | --- |
| Provider | A named upstream connection built on a channel: base URL and endpoint overrides, typed settings the channel declares, a credential pool with a `round_robin` or `sticky` strategy, an optional proxy and TLS fingerprint. |
| Credential | One secret in a provider's pool: an API key, an OAuth token pair, a session cookie, or service-account material. Carries a weight, RPM/TPM limits, proxy and fingerprint overrides, and an enable flag. Health is tracked per credential and model. |
| Channel | The built-in adapter for one upstream family: how to authenticate, which paths exist, how tokens refresh, which routing defaults to seed. The binary ships 28 channel ids, from `openai` and `claudeapi` to `codex`, `aistudio`, `aws-bedrock`, and `custom`. |
| Model | An upstream model recorded under a provider, with display name, context window, max output, thinking flags, and variants. Pulled from the provider or entered by hand. |
| Route | A public model entry with members, each a provider plus an upstream model, ordered by tier (failover level) and weight (split inside a tier). The console calls routes load balancers. |
| Alias | Another incoming model name that resolves to a target name, globally or for one provider, before routing. |
| Variant | A suffix form of an exposed model, such as a thinking level or `-tier-*`, that maps to the base model and injects request fields. |
| User API key | The key a client sends. It belongs to a user, who may belong to a team and an organization. Permissions, rate limits, and quotas attach at any of those scopes and are inherited downward. |
| Rule set | An ordered, reusable list of `system_text`, `cache_breakpoint`, `rewrite`, `transform`, and `header` rules applied to the provider-native request and response. Attached to providers; creating a provider also creates an empty private set for it. |
| Routing rule | A per-provider decision keyed by operation and inbound protocol: pass through, transform to a target format, answer locally, or reject as unsupported. Channels seed defaults. |

## Deployment Choices

- **Native binary.** One `gproxy` executable with the console embedded, for a
  server, a desktop, or a phone. Installers for Linux, macOS, Windows, and
  Android; portable archives for the same targets.
- **Container.** `ghcr.io/leenhawk/gproxy:<tag>`, the same binary on
  `linux/amd64`, with its data directory at `/var/lib/gproxy`.
- **Edge wasm.** Prebuilt bundles for Cloudflare Workers, Deno Deploy, and
  Netlify Edge. libSQL holds the configuration; the console is served by the
  platform's static layer.
- **Embedding.** `gproxy-core` is a Rust library. Another application can
  link it and call the same execute surface the hosts use. The crates are not
  published; embedding means a path or git dependency on this repository.

All four run the same core and the same admin model. Platform limits still
apply: edge deployments require libSQL, run as a single isolate unless a
shared cache is configured, and do not offer the Claude Web channel.

## Coming From v2

v3 keeps the job and most of the vocabulary but is a rewrite. The TOML
configuration file is gone: configuration is command-line flags, `GPROXY_*`
environment variables, and optional `.env` files. A live v2 SQLite database is
imported with `gproxy migrate --from-v2 <path>`, which performs a dry run
unless `--apply` is given. Read [v2 to v3 Migration](/deployment/v2-to-v3/)
before pointing v3 at a v2 data directory.

## What GPROXY Does Not Do

GPROXY does not host models or run inference. It is not a generic reverse
proxy either: it parses LLM request bodies, rewrites streams, extracts token
usage, and manages provider-specific authentication. The console and portal
are part of your deployment. GPROXY binds to `127.0.0.1` by default; exposing
it, backing up the data directory, and guarding the master key are your
responsibility.

## Next Steps

- [Downloads](/getting-started/downloads/) and
  [Installation](/getting-started/installation/).
- [Quick Start](/getting-started/quick-start/) for the first working route.
- [Providers & Credentials](/guides/providers/) and
  [Models, Routes & Aliases](/guides/models/).
- [Architecture](/introduction/architecture/) for the request lifecycle in
  detail.
