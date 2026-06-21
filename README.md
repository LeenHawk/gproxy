# gproxy

**A high-performance, multi-provider LLM proxy in a single Rust binary** — with
an embedded React console, multi-tenant auth, and the same engine compiled to
run **natively, in Docker, or on the serverless edge (WebAssembly)**.

- 🪪 **License:** AGPL-3.0-or-later
- 🐳 **Image:** `ghcr.io/leenhawk/gproxy`
- 🦀 **Build targets:** native binary · Docker · edge wasm (Cloudflare / Deno / Netlify / Supabase / Vercel / EdgeOne) · Appwrite Functions
- 🌐 **Console:** built in, served at `/console`

---

## What it does

gproxy exposes a unified **OpenAI / Anthropic / Gemini-compatible** HTTP surface
on top of many upstream LLM providers, and adds everything you need to run it as
a shared service:

- **Multi-provider routing** — OpenAI, Anthropic, Gemini/Vertex, DeepSeek, Groq,
  OpenRouter, NVIDIA, Vercel AI Gateway, Claude Code, Codex, and any
  OpenAI-compatible custom endpoint.
- **Two routing modes** — aggregated `/v1/...` (provider encoded in the model
  name) and scoped `/{provider}/v1/...` (provider in the URL).
- **Cross-protocol translation** — an OpenAI client can talk to a Claude upstream
  (and vice-versa) through the `transform` layer; same-dialect requests take a
  minimal-parsing fast path.
- **Multi-tenant auth** — users, API keys, glob model permissions, RPM/RPD/token
  rate limits, and USD-denominated quotas.
- **Claude prompt caching, rewrite rules, circuit breakers, credential pools.**
- **Pluggable storage** — SQLite / PostgreSQL / MySQL, with optional
  XChaCha20-Poly1305 at-rest encryption.
- **Embedded console** — no separate frontend to deploy; it's in the binary.

---

## Deploy

### 🐳 One-click (Docker — recommended)

The container is **fully self-contained**: embedded console, file-based SQLite,
no external services required. This is the simplest way to run gproxy.

[![Deploy to Koyeb](https://www.koyeb.com/static/images/deploy/button.svg)](https://app.koyeb.com/deploy?type=docker&image=ghcr.io/leenhawk/gproxy&ports=8787;http;/&name=gproxy&env[GPROXY_ADMIN_PASSWORD]=change-me)
[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/LeenHawk/gproxy)
[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/new)

Or run it yourself (Docker or Podman):

```bash
docker run -p 8787:8787 -e GPROXY_ADMIN_PASSWORD=change-me ghcr.io/leenhawk/gproxy
# then open http://localhost:8787/console  (log in as admin / change-me)
```

Multi-arch images are published for `amd64` + `arm64`, in both glibc and
`-musl` flavours, on every release.

### ☁️ Serverless edge (WebAssembly)

The **same** router runs as a wasm edge function on six platforms. Bundles are
built per release; deployment uses each platform's CLI (edge functions need an
external **Turso** control-plane DB + optional **Upstash** cache). Full
walkthrough — and the exact commands — in **[docs/edge-deploy.md](docs/edge-deploy.md)**.

| Platform | Notes |
|---|---|
| Cloudflare Workers | `wrangler deploy` |
| Deno Deploy | `deploy/deno/build.sh` (build + deploy) |
| Netlify Edge | `netlify deploy` |
| Supabase Edge | Docker/eszip path (`--network-id host`) |
| Tencent EdgeOne Pages | `edgeone pages deploy` |
| Vercel Edge | needs a Pro plan (bundle > 1 MB Hobby limit) |

### 🦀 Appwrite Functions (Rust-native)

gproxy can run as an Appwrite **Rust 1.83** function (built from source) via the
adapter in **[deploy/appwrite/](deploy/appwrite/)**. See its
[NOTES](deploy/appwrite/NOTES.md) for setup.

### 📦 Native binary

Pre-built binaries (linux/macOS/windows, x86_64 + aarch64) ship on every
[release](https://github.com/LeenHawk/gproxy/releases). Or build it:

```bash
git clone https://github.com/LeenHawk/gproxy.git && cd gproxy
cargo build --release          # binary at target/release/gproxy
./target/release/gproxy        # http://127.0.0.1:8787/console
```

---

## Configure

gproxy is configured by **environment variables** (no config file). The control
plane then lives in the database and is managed through `/console`.

| Variable | Default | Purpose |
|---|---|---|
| `GPROXY_HOST` / `GPROXY_PORT` | `127.0.0.1` / `8787` | bind address |
| `GPROXY_PERSISTENCE` | `file` | `file` (SQLite under `GPROXY_DATA_DIR`) or `db` |
| `GPROXY_DSN` | — | DSN when `persistence=db` (Postgres/MySQL/SQLite) |
| `GPROXY_MASTER_KEY` | — | unseal stored secrets (absent = plaintext) |
| `GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` | `admin` / random | first-boot admin |

**Upgrading from v1?** Point a v2 binary at your existing v1 SQLite database and
it migrates in place on first boot (backing the old file up as `*.v1.bak`).

---

## First request

```bash
# Aggregated — provider/model in the body
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer <your-key>" -H "Content-Type: application/json" \
  -d '{"model":"openai-main/gpt-4.1-mini","messages":[{"role":"user","content":"Hello"}]}'

# Scoped — provider in the URL
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer <your-key>" -H "Content-Type: application/json" \
  -d '{"model":"gpt-4.1-mini","messages":[{"role":"user","content":"Hello"}]}'
```

Ops endpoints (`/healthz`, `/version`, `/metrics`) are admin-gated.

---

## Documentation

- **[Edge deployment](docs/edge-deploy.md)** — cargo-free deploy to all six edge platforms
- **[Architecture](docs/architecture-design.md)** — the v2 design
- **[Developer guide](docs/developers/README.md)** — build, layout, contributing

## License

[AGPL-3.0-or-later](LICENSE) · Author: [LeenHawk](https://github.com/LeenHawk)
