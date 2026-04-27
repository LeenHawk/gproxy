# GPROXY

**A high-performance LLM proxy server written in Rust.** Multi-provider,
multi-tenant, with an embedded React console — all in a single static
binary.

- 📘 **Documentation:** <https://gproxy.leenhawk.com>
- 📦 **Downloads:** <https://gproxy.leenhawk.com/downloads/>
- 🦀 **Crate:** `gproxy-sdk`
- 🪪 **License:** AGPL-3.0-or-later
- 🌐 **Languages:** English · [简体中文](./README.zh_CN.md)

---

## What it does

GPROXY exposes a unified, **OpenAI / Anthropic / Gemini compatible** HTTP
surface on top of many upstream LLM providers, and adds the primitives
you need to run it as a shared service:

- **Multi-provider routing** — OpenAI, Anthropic, Vertex / Gemini,
  DeepSeek, Groq, OpenRouter, NVIDIA, Claude Code, Codex, Antigravity,
  and any OpenAI-compatible custom endpoint.
- **Two routing modes** — aggregated `/v1/...` (provider encoded in the
  model name) and scoped `/{provider}/v1/...` (provider in the URL).
- **Same-protocol passthrough** — minimal-parsing fast path when the
  client and upstream speak the same dialect.
- **Cross-protocol translation** — an OpenAI client can route to a
  Claude upstream (and vice versa) through the protocol `transform`
  layer.
- **Multi-tenant auth** — users, API keys, glob model permissions,
  RPM / RPD / token rate limits, and USD-denominated quotas.
- **Claude prompt caching** — server-side `cache_breakpoint` rules and
  magic-string triggers for `anthropic` / `claudecode` channels.
- **Request & message rewrite rules** — JSON-field manipulation on the
  request body, plus regex text substitution on message content.
- **Embedded React console** — built into the binary, mounted at
  `/console`. No separate frontend to deploy.
- **Pluggable storage** — SQLite, PostgreSQL, MySQL via SeaORM / SQLx,
  with optional XChaCha20-Poly1305 at-rest encryption.
- **Rust SDK** — `gproxy-sdk` re-exports the protocol, routing, and
  provider crates so you can embed the engine into your own service.

## Quick start

**Download from releases if you use a common device and common os!**

```bash
# 1. Build
git clone https://github.com/LeenHawk/gproxy.git
cd gproxy
cargo build -p gproxy --release

# 2. Run with a minimal config
GPROXY_CONFIG=./gproxy.toml ./target/release/gproxy
```

A minimal `gproxy.toml` seed that creates an admin user with wildcard
permissions:

> **Note:** `gproxy.toml` is only read **once**, on first launch when the
> database does not yet exist. After the initial seed, the database
> becomes the single source of truth — subsequent edits to
> `gproxy.toml` are ignored. Manage live configuration through the
> `/console` UI (or the admin API). To re-seed from TOML, delete the
> database file first.

```toml
[global]
host = "127.0.0.1"
port = 8787
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"

[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [ { api_key = "sk-your-upstream-key" } ]

[[models]]
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true

[[users]]
name = "admin"
password = "change-me"
is_admin = true
enabled = true

[[users.keys]]
api_key = "sk-admin-1"
label = "default"
enabled = true

[[permissions]]
user_name = "admin"
model_pattern = "*"
```

Then open <http://127.0.0.1:8787/console> and log in as `admin`.

Full walkthrough: **[Quick Start](https://gproxy.leenhawk.com/getting-started/quick-start/)**.

## Sending your first request

```bash
# Aggregated endpoint — provider/model prefix in the body
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai-main/gpt-4.1-mini",
    "messages": [ { "role": "user", "content": "Hello" } ]
  }'

# Scoped endpoint — provider in the URL, raw upstream model id in the body
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [ { "role": "user", "content": "Hello" } ]
  }'
```

See **[First Request](https://gproxy.leenhawk.com/getting-started/first-request/)**
for Anthropic and Gemini examples.

## Repository layout

```text
apps/                  # Runnable binaries
  gproxy/              # Main binary (HTTP server + embedded console)
  gproxy-recorder/     # Upstream traffic recorder (dev/debugging)
crates/                # Server-side crates composed by the binary
  gproxy-core/         # Config, identity, policy, quota, routing types
  gproxy-storage/      # SeaORM storage + at-rest encryption + schema sync
  gproxy-api/          # Admin + user HTTP API, auth, login, CORS
  gproxy-server/       # The Axum server wiring it all together
sdk/                   # Framework-agnostic libraries (no DB/HTTP dependencies)
  gproxy-protocol/     # L0: OpenAI/Claude/Gemini wire types + transforms
  gproxy-channel/      # L1: Channel trait, channel implementations,
                       #     credentials, billing, utils, health
  gproxy-engine/       # L2: GproxyEngine, ProviderStore, routing,
                       #     retry, credential affinity, backends
  gproxy-sdk/          # Umbrella crate re-exporting the three layers above
frontend/console/      # React console, embedded into the binary at build time
docs/                  # Starlight documentation site (source for gproxy.leenhawk.com)
```

## Documentation

The full documentation lives at **<https://gproxy.leenhawk.com>**. Some
entry points:

- [What is GPROXY?](https://gproxy.leenhawk.com/introduction/what-is-gproxy/)
- [Architecture](https://gproxy.leenhawk.com/introduction/architecture/)
- [Installation](https://gproxy.leenhawk.com/getting-started/installation/)
- [Providers & Channels](https://gproxy.leenhawk.com/guides/providers/)
- [Models & Aliases](https://gproxy.leenhawk.com/guides/models/)
- [Permissions, Rate Limits & Quotas](https://gproxy.leenhawk.com/guides/permissions/)
- [Request Rewrite Rules](https://gproxy.leenhawk.com/guides/rewrite-rules/) · [Message Rewrite Rules](https://gproxy.leenhawk.com/guides/message-rewrite/)
- [Claude Prompt Caching](https://gproxy.leenhawk.com/guides/claude-caching/)
- [Adding a Channel](https://gproxy.leenhawk.com/guides/adding-a-channel/)
- [Routing Table](https://gproxy.leenhawk.com/reference/routing-table/)
- [Environment Variables](https://gproxy.leenhawk.com/reference/environment-variables/) · [TOML Config](https://gproxy.leenhawk.com/reference/toml-config/)
- [Rust SDK](https://gproxy.leenhawk.com/reference/sdk/)

To run the docs locally:

```bash
cd docs
pnpm install
pnpm dev
```

## License

Released under the [AGPL-3.0-or-later](./LICENSE) license.

Author: [LeenHawk](https://github.com/LeenHawk)
