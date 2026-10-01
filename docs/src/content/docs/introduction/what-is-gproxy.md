---
title: "What is GPROXY?"
description: "Learn what GPROXY does, which protocols it supports, and the main configuration concepts."
---


GPROXY is a self-hosted LLM API gateway. Once you connect your upstream accounts, clients need only a gateway URL, a gateway API key, and a model name.

It selects providers and credentials, converts request and response formats when needed, enforces access rules and limits, and records usage and costs. GPROXY does not run models; the upstream service performs inference.

## When to use it

- Manage several upstream accounts with centralized credentials and failover.
- Give a team stable model names so provider changes do not require editing every client.
- Connect Codex CLI, Claude Code, and similar tools through one gateway with per-user usage records.
- Embed gateway functionality in a Rust application; see [Embedding the core](/reference/embedding/).

## Supported protocols

The main formats are OpenAI Chat Completions, OpenAI Responses, Claude Messages, and Gemini GenerateContent, including streaming. GPROXY translates directly between the client and upstream formats when they differ.

There are also interfaces for embeddings, reranking, images, audio, video, files, and Realtime. Their availability depends on the channel, model, and deployment. Format conversion cannot add capabilities the upstream does not have. See [Routing and endpoints](/reference/routing-table/) for the interface list.

## Key concepts

| Concept | Meaning |
| --- | --- |
| Channel | An adapter for an upstream family, defining authentication, formats, and supported operations; for example `openai`, `codex`, or `custom`. |
| Provider | A saved upstream connection with a name, channel, address, configuration, and credential pool. |
| Credential | An upstream API key, OAuth token, or Cookie. A provider can hold several credentials. |
| Model route | A client-facing model name with one or more provider/model members, weights, and fallback tiers. |
| Gateway API key | A key issued by GPROXY, associated with a user, permissions, and budgets. It is separate from an upstream key. |
| Rewrite rule | A rule for system text, cache breakpoints, JSON fields, text, or headers. |

Secrets are encrypted when a master key is configured. Without one, the CLI stores secrets unencrypted and prints a warning. See [Configuration](/reference/configuration/).

## Deployment options

| Option | Use |
| --- | --- |
| Application | Desktop or mobile use, with an in-app setup wizard and console. |
| CLI / container | Server deployment with HTTP APIs and a browser console. |
| Cloudflare Workers | Edge deployment with remote storage and WebSocket / Realtime support. |

See [Installation](/getting-started/installation/) for packages and platform restrictions. When updating from v3, read the [migration guide](/deployment/v3-to-v4/) first.

## Model names and request URLs

With a route named `fast`, send `model: "fast"` to `/v1/chat/completions`.

To call a provider directly, use `/openai-main/v1/chat/completions` and supply its upstream model ID. A namespaced route such as `acme/fast` can also be called through `/acme/v1/chat/completions` with `model: "fast"`.

See [Models and routes](/guides/models/) for resolution order and naming restrictions, or follow [Quick start](/getting-started/quick-start/) for your first setup.
