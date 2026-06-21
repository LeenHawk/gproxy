# GPROXY

**A high-performance, multi-provider LLM proxy in a single Rust binary** — with
an embedded React console, multi-tenant auth, and the same engine compiled to run
**natively, in Docker, or on the serverless edge (WebAssembly)**.

English · [简体中文](README.zh_CN.md)

- 🪪 **License:** AGPL-3.0-or-later · 🐳 **Image:** `ghcr.io/leenhawk/gproxy`
- 🦀 **Targets:** native binary · Docker · edge wasm (Cloudflare / Deno / Netlify / Supabase / EdgeOne / Appwrite)
- 🖥️ **Console:** built in, served at `/console`

---

## What it does

GPROXY exposes a unified **OpenAI / Anthropic / Gemini-compatible** HTTP surface
on top of many upstream LLM providers, and adds everything you need to run it as a
shared service:

- **Multi-provider routing** — OpenAI, Anthropic, Gemini/Vertex, DeepSeek, Groq,
  OpenRouter, NVIDIA, Vercel AI Gateway, Claude Code, Codex, and any
  OpenAI-compatible custom endpoint.
- **Two routing modes** — aggregated `/v1/...` (provider in the model name) and
  scoped `/{provider}/v1/...` (provider in the URL).
- **Cross-protocol translation** — an OpenAI client can talk to a Claude upstream
  (and vice-versa); same-dialect requests take a minimal-parsing fast path.
- **Multi-tenant auth** — users, API keys, glob model permissions, RPM/RPD/token
  rate limits, USD quotas. Claude prompt caching, rewrite rules, circuit breakers.
- **Pluggable storage** — SQLite / PostgreSQL / MySQL, optional at-rest encryption.
- **Embedded console** — no separate frontend to deploy.

---

## Deploy

### 🐳 One-click (Docker — recommended)

Fully self-contained: embedded console, file-based SQLite, no external services.

[![Deploy to Koyeb](https://www.koyeb.com/static/images/deploy/button.svg)](https://app.koyeb.com/deploy?type=docker&image=ghcr.io/leenhawk/gproxy&ports=8787;http;/&name=gproxy&env[GPROXY_ADMIN_PASSWORD]=change-me)
[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/LeenHawk/gproxy)

```bash
docker run -p 8787:8787 -e GPROXY_ADMIN_PASSWORD=change-me ghcr.io/leenhawk/gproxy
# then open http://localhost:8787/console  (admin / change-me)
```

### ☁️ Serverless edge (WebAssembly)

The same router runs as a wasm edge function on six platforms. Prebuilt,
ready-to-deploy bundles live on the [**`deploy` branch**](https://github.com/LeenHawk/gproxy/tree/deploy)
(no toolchain needed — the platforms have no cargo). Edge functions need an
external **Turso** control-plane DB (+ optional **Upstash** cache); full
walkthrough in **[docs/edge-deploy.md](docs/edge-deploy.md)**.

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https://github.com/LeenHawk/gproxy&branch=deploy&create_from_path=netlify)

| Platform | Bundle | Deploy |
|---|---|---|
| Cloudflare Workers | [`deploy/cloudflare`](https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare) | one-click button ☝️ / `wrangler deploy` |
| Netlify Edge | [`deploy/netlify`](https://github.com/LeenHawk/gproxy/tree/deploy/netlify) | one-click button ☝️ / `netlify deploy --prod` |
| Deno Deploy | — | `deploy/deno/build.sh` (CLI) |
| Supabase Edge | [`deploy/supabase`](https://github.com/LeenHawk/gproxy/tree/deploy/supabase) | `supabase functions deploy gproxy` (Docker/eszip, CLI) |
| EdgeOne Pages | [`deploy/eopages`](https://github.com/LeenHawk/gproxy/tree/deploy/eopages) | `edgeone pages deploy` (CLI) |
| **Appwrite Functions** | [`deploy/appwrite-deno`](https://github.com/LeenHawk/gproxy/tree/deploy/appwrite-deno) | `appwrite push functions` (deno-2.0, CLI) |

### 📦 Native binary

Pre-built binaries (linux/macOS/windows, x86_64 + aarch64) ship on every
[release](https://github.com/LeenHawk/gproxy/releases). Or `cargo build --release`.

---

## Configure

GPROXY is configured by **environment variables**; live config then lives in the
database and is managed through `/console`.

| Variable | Default | Purpose |
|---|---|---|
| `GPROXY_HOST` / `GPROXY_PORT` | `127.0.0.1` / `8787` | bind address |
| `GPROXY_PERSISTENCE` | `file` | `file` (SQLite under `GPROXY_DATA_DIR`) or `db` |
| `GPROXY_DSN` | — | DSN when `persistence=db` (Postgres/MySQL/SQLite) |
| `GPROXY_MASTER_KEY` | — | unseal stored secrets (absent = plaintext) |
| `GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` | `admin` / random | first-boot admin |

**Upgrading from v1?** Point a v2 binary at your existing v1 SQLite database and it
migrates in place on first boot (backing the old file up as `*.v1.bak`).

---

## First request

```bash
# Aggregated — provider/model in the body
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer <your-key>" -H "Content-Type: application/json" \
  -d '{"model":"openai-main/gpt-4.1-mini","messages":[{"role":"user","content":"Hello"}]}'
```

Ops endpoints (`/healthz`, `/version`, `/metrics`) are admin-gated.

## Documentation

- **[Edge deployment](docs/edge-deploy.md)** · **[Architecture](docs/architecture-design.md)** · **[Developer guide](docs/developers/README.md)**

## License

[AGPL-3.0-or-later](LICENSE) · Author: [LeenHawk](https://github.com/LeenHawk)
