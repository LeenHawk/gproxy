---
title: What is GPROXY v2?
description: A practical overview of GPROXY, the clients it accepts, and the problems it solves.
---

**GPROXY v2** is a self-hosted gateway for LLM APIs. Your applications call one
endpoint, while GPROXY chooses an upstream provider, converts the request when
the API formats differ, applies access and spending policies, and records usage.
The embedded console gives operators one place to manage providers,
credentials, models, users, and logs.

Use GPROXY when you want to:

- keep application code independent from a single model provider;
- expose OpenAI, Claude, or Gemini-compatible endpoints to existing clients;
- route one public model name across multiple upstream accounts or providers;
- give different users access to different models, limits, and budgets;
- add prompt caching, request rewrites, failover, and usage accounting at the
  gateway instead of rebuilding them in every application.

## How a Request Is Handled

1. The client sends an OpenAI, Claude, or Gemini-compatible request.
2. GPROXY authenticates the API key and checks model permissions and limits.
3. The model name resolves to a provider, upstream model, and usable credential.
4. If the upstream uses another API format, GPROXY converts the request and the
   response. Provider rules and cache breakpoints are applied after conversion.
5. GPROXY records usage, updates quotas, and returns the response in the format
   expected by the client.

Requests can use the aggregated `/v1/...` API, where the model name selects the
route, or a scoped `/{provider}/v1/...` API, where the URL selects one provider.
Aggregated routing is the normal application-facing mode. Scoped routing is
useful for provider-specific clients and troubleshooting.

## Core Concepts

| Concept | What it means |
| --- | --- |
| Provider | A named upstream connection, such as OpenAI, Anthropic, OpenRouter, Vercel, Codex, or a custom endpoint. |
| Credential | An API key, OAuth token, service account, or session used by a provider. A provider can have a credential pool. |
| Model | An upstream model discovered from a provider or entered manually, with optional pricing metadata. |
| Route | A public model entry that can select one or more provider/model members. |
| Alias | Another public model name that resolves to a route. |
| User API key | The key a client sends to GPROXY, with its own permissions, limits, and quota. |
| Rule set | Reusable request rules for system text, cache breakpoints, JSON rewrites, text transforms, and headers. |

## Deployment Choices

- **Native binary:** the simplest choice for a VM or bare-metal server.
- **Docker:** includes the API and console in one container and works well for
  most self-hosted deployments.
- **Serverless edge:** runs the gateway on supported edge platforms with Turso
  for persistent configuration and optional Upstash caching.

All three options expose the same API and administration model. Platform limits
still apply: for example, edge deployments cannot use a native outbound proxy
or local SQLite file in the same way as a long-running server.

## Coming From v1

v2 keeps the same job but has a different configuration and persistence model.
On a native deployment, it can import supported control-plane data from an
existing v1 SQLite database and preserve the old file as a backup. Read the
[v1 to v2 migration guide](/deployment/v1-to-v2/) before replacing a running
instance.

## What GPROXY Does Not Do

GPROXY does not host models or run inference. It is also more than a generic
reverse proxy: it understands LLM request formats, model routing, streaming,
tool calls, token usage, and provider-specific authentication. The console is
part of your own deployment, so you remain responsible for network exposure,
backups, and operational access.

## Next Steps

- [Install GPROXY](/getting-started/installation/) or follow the
  [quick start](/getting-started/quick-start/).
- Learn how to add [providers and credentials](/guides/providers/).
- Configure [models and aliases](/guides/models/).
- Add [prompt cache breakpoints](/guides/claude-caching/).
- Read the [architecture guide](/introduction/architecture/) when you need the
  implementation details.
