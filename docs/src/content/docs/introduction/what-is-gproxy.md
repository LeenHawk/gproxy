---
title: What is GPROXY?
description: What GPROXY v4 does, who it is for, how a request moves through it, and the vocabulary you will meet everywhere else in this site.
---

**GPROXY** is a self-hosted gateway for LLM APIs. Your clients call one base
URL with one API key. GPROXY decides who is calling and what they may reach,
resolves the model name to an upstream provider and one of its credentials,
converts the request when the client and the upstream speak different wire
formats, spends the caller's budget, and records what the exchange cost.

v4 is a rewrite. The job is the same as v3's; the shape is not. The engine is
now an embeddable library with no server in it, the product rules sit in a
second library with no transport in it, and a *host* is the thin thing that
binds one to a socket. There are three hosts, and they share one route table
and one set of decisions.

## Who It Is For

- Operators who hold several upstream accounts and want one pool behind a
  single endpoint, with failover and per-credential health tracking.
- Teams that want application code to depend on one model name rather than on
  one vendor's SDK.
- Users of the Codex CLI or Claude Code who want those tools to run against
  pooled credentials with every request metered.
- Rust applications that want the same pooled-credential discipline without
  running a gateway at all — see [Embedding the Core](/reference/embedding/).

## Accepted Wire Formats

GPROXY accepts OpenAI Responses, OpenAI Chat Completions, Claude Messages and
Gemini GenerateContent. A request in any of these can be served by an upstream
that speaks another: conversion is **direct between the two formats, with no
intermediate representation**. Upstream specifications change faster than any
pivot format can track, and conversion fidelity is the product.

A Claude Messages request served by an OpenAI Chat upstream comes back as
Claude Messages, usage included:

```json
{"type":"message","id":"chatcmpl-…","content":[{"type":"text","text":"…"}],
 "model":"gpt-4o-mini","role":"assistant","stop_reason":"end_turn",
 "usage":{"input_tokens":11,"output_tokens":7}}
```

Beyond content generation the same path carries embeddings, reranking, images,
audio, video, files, token counting, model listing, conversations, moderation
and realtime sockets. Streams are SSE, a WebSocket, or Gemini's incremental
JSON array, whichever the caller asked for.

## How a Request Is Handled

```text
transport (a host)
  → auth        Caller   { user, role, api key, organization, team, subscription }
  → admission   Admitted { allowed providers, allowed credentials, budget owners,
                           scope, session identity, rate-limit permit }
  → the engine  resolves the model, walks the plan, converts, sends, settles
```

Each step narrows, and **nothing below re-opens what was narrowed above**. The
engine is handed a provider set and a credential set, never a caller to
interpret, so it cannot re-answer a question the layer above already answered.

Admission runs in a fixed order — permissions, credential visibility, the
budget chain, the scope, the session identity, then rate limits. Rate limits
are last because they are the only step that *consumes* something: a request
refused for permissions must not move a shared counter.

## Three Ways to Address the Gateway

One instance answers the same upstream API at three mounts.

| Path | Mount | Narrows to |
| --- | --- | --- |
| `/v1/messages` | aggregated | nothing |
| `/acme/v1/messages` | namespace `acme` | the exposed names under `acme/` |
| `/openai-prod/v1/messages` | provider `openai-prod` | that provider |

A **namespace** is the first segment of a slash-bearing exposed model name, so
exposing `acme/fast` creates the namespace `acme`. A **provider** mount names a
`providers.name` — the operator's label, not the row id, because nobody types a
machine-minted id into a client's base URL. A namespace wins over a provider of
the same name.

A prefix is only stripped when what is left is **also a surface this gateway
serves**. An ambiguous path therefore resolves towards the aggregated mount:
losing a mount is a 404 an operator can see, while taking one that was not
meant is a request sent to the wrong upstream.

## Core Concepts

| Concept | What it means |
| --- | --- |
| Channel | The compiled-in adapter for one upstream family: its URLs, how a credential is injected, which dialects it speaks, how its stream reports usage, and how to log in and refresh. 25 of them ship, from `openai` and `claudeapi` to `codex`, `kiro` and `custom`. |
| Provider | One saved connection on a channel: a name, an optional base URL, the channel's own `config` JSON, and a pool of credentials. |
| Credential | One secret in that pool — an API key, an OAuth token pair, a session cookie, service-account material — sealed at rest, with a lifecycle status and an owner. |
| Route | An ordered set of members, each a provider plus an upstream model, with a `tier` (the failover level) and a `weight` (the split inside a tier). |
| Exposed model | The public name a client sends as `model`. It points at a route. A name with a `/` in it creates a namespace. |
| Gateway API key | What a client presents. It belongs to a user and may be bound to an organization, a team and a subscription; that binding is what decides which credentials it can reach and whose budget it spends. |
| Rewrite rule | An ordered regex replacement over body text, a named header value or a named query value, filtered by operation, model, inbound header or stream event. |
| Operation rule | A per-provider override of what a channel does for one operation. Channel defaults stay in code. |

## The Three Hosts

| Host | What it is |
| --- | --- |
| `gproxy` | The native server: an axum router over the product layer, on a socket. This is what an operator runs. |
| `gproxy-desktop` | A Tauri window over the same instance, with a data-plane-only HTTP socket on `127.0.0.1` for the CLIs that cannot speak IPC. **New in v4.** |
| `gproxy-host-edge` | A Cloudflare Workers `fetch` handler that mounts the *same* axum router. **New in v4.** |

The Workers host contains no route table of its own: a route the native host
grows is a route it serves, with no edit there. See
[Edge (Cloudflare Workers)](/deployment/edge/).

## What GPROXY Does Not Do

GPROXY does not host models or run inference. It is not a generic reverse proxy
either: it parses LLM request bodies, rewrites streams, extracts token usage and
manages provider-specific authentication. It binds to `127.0.0.1` by default;
exposing it, backing up the data directory and guarding the master key are
yours.

There is also no release pipeline yet. v4 is built from source — there are no
installers, no published container image and no signed artifacts. See
[Installation](/getting-started/installation/).

## Next Steps

- [Installation](/getting-started/installation/) and
  [Quick Start](/getting-started/quick-start/).
- [Architecture](/introduction/architecture/) for the crates and the seams.
- [Providers & Credentials](/guides/providers/) and
  [Models, Routes & Exposed Names](/guides/models/).
