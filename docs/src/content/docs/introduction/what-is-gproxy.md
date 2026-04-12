---
title: What is GPROXY?
description: A high-level overview of the GPROXY LLM proxy server and what it is designed to do.
---

**GPROXY** is a high-performance LLM proxy server written in Rust. It exposes a
unified, OpenAI / Anthropic / Gemini compatible HTTP surface on top of many
upstream providers, while adding the primitives you need to run it as a shared
service: users, API keys, model permissions, rate limits, cost quotas, usage
logging, and an embedded browser console.

It ships as a **single static binary** (with an embedded React console) and an
optional **Rust SDK** for developers who want to reuse the engine in their own
applications.

## What it is good at

- **Fanning out to many upstreams from one endpoint.** A single GPROXY instance
  can route to OpenAI, Anthropic, Vertex / Gemini, DeepSeek, Groq, OpenRouter,
  NVIDIA, Claude Code, Codex, Antigravity, custom OpenAI-compatible endpoints,
  and more — each configured as an independent *provider*.
- **Multi-tenant access control.** Issue API keys to individual users, gate
  them with glob-style model permissions, apply RPM / RPD / token rate limits
  per model pattern, and enforce USD-denominated quotas with a reconciler
  running in the background.
- **Cross-protocol translation.** A client speaking the OpenAI Chat
  Completions format can be routed to an Anthropic or Gemini upstream (and
  vice versa) through the protocol `transform` layer.
- **Same-protocol passthrough.** When the client and upstream speak the same
  protocol, GPROXY forwards bytes with minimal parsing for low-overhead,
  high-throughput operation.
- **Operational visibility.** Structured upstream / downstream logs (with
  optional body capture), per-request usage accounting, model health tracking,
  and a web console that surfaces all of it.

## What it is not

- It is **not a model host.** GPROXY does not run inference itself; it talks
  to real upstream providers over HTTP.
- It is **not a load balancer for web traffic generally.** It understands
  LLM protocols (OpenAI, Claude, Gemini) and is optimized for them.
- It does **not ship a managed UI behind SSO.** Authentication for the
  embedded console is a username + password that issues a bearer session
  token; integrate it behind your own reverse proxy if you need more.

## Core concepts at a glance

| Concept | What it means in GPROXY |
| --- | --- |
| **Provider** | A configured upstream (name + channel + settings + credentials). |
| **Channel** | The code that speaks a specific upstream protocol (OpenAI, Anthropic, Gemini, …). |
| **Model** | A forwardable model id on a provider. May carry pricing. |
| **Alias** | A friendly name that resolves to a real `(provider, model)` pair. |
| **User** | An account with one or more API keys, permissions, limits, and quota. |
| **Permission** | A `(user, provider, model_pattern)` tuple granting routable access. |
| **Rate limit** | RPM / RPD / token ceilings scoped to a user + model pattern. |
| **Quota** | A cost ceiling (USD) enforced across all usage for a user. |

## Where to go next

- **Install and run** — [Installation](/getting-started/installation/)
- **Boot a working config in 5 minutes** — [Quick Start](/getting-started/quick-start/)
- **Understand the internals** — [Architecture](/introduction/architecture/)
