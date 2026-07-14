# GPROXY

Run OpenAI, Anthropic, and Gemini-compatible clients through one gateway.
GPROXY handles provider routing, protocol conversion, credentials, quotas, and
observability, with an embedded console for day-to-day administration. Deploy it
as a native binary, a Docker container, or a serverless edge function.

English · [简体中文](README.zh_CN.md)

[![GitHub Sponsors](https://img.shields.io/github/sponsors/LeenHawk?logo=githubsponsors&label=Sponsor)](https://github.com/sponsors/LeenHawk)

- 🪪 **License:** AGPL-3.0-or-later · 🐳 **Image:** `ghcr.io/leenhawk/gproxy`
- 🦀 **Targets:** native binary · Docker · edge wasm (Cloudflare / Deno / Netlify / Supabase / EdgeOne / Appwrite)
- 🖥️ **Console:** built in, served at `/console`

---

## What it does

GPROXY gives your applications one stable API while letting you choose and
combine upstream providers behind it:

- **Multi-provider routing** — OpenAI, Anthropic, Gemini/Vertex, DeepSeek, Groq,
  OpenRouter, NVIDIA, Vercel AI Gateway, Claude Code, Codex, Grok Build, and any
  OpenAI-compatible custom endpoint.
- **Two routing modes** — aggregated `/v1/...` (provider in the model name) and
  scoped `/{provider}/v1/...` (provider in the URL).
- **Cross-protocol translation** — an OpenAI client can use a Claude or Gemini
  upstream, and responses are converted back to the format the client expects.
- **Multi-tenant auth** — users, API keys, glob model permissions, RPM/RPD/token
  rate limits, and USD quotas.
- **Prompt and request controls** — Claude and OpenAI cache breakpoints, reusable
  rewrite rules, credential failover, and circuit breakers.
- **Pluggable storage** — SQLite / PostgreSQL / MySQL, optional at-rest encryption.
- **Embedded console** — no separate frontend to deploy.

---

## Deploy

### 🐳 Docker (recommended)

Fully self-contained: embedded console, local file storage, no external services.

[![Deploy to Koyeb](https://www.koyeb.com/static/images/deploy/button.svg)](https://app.koyeb.com/deploy?type=docker&image=ghcr.io/leenhawk/gproxy&ports=8787;http;/&name=gproxy&env[GPROXY_ADMIN_PASSWORD]=change-me)
[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/LeenHawk/gproxy)

```bash
docker run -p 8787:8787 -e GPROXY_ADMIN_PASSWORD=change-me ghcr.io/leenhawk/gproxy
# then open http://localhost:8787/console  (admin / change-me)
```

> Set your own admin password before exposing the service. The container refuses
> to start when `GPROXY_ADMIN_PASSWORD` is empty or contains only whitespace.
>
> **Plain HTTP console access** works for same-origin deployments, including LAN
> IPs, server IPs, and tunnels. Use HTTPS when exposing GPROXY beyond local
> development; cross-site console deployments still require HTTPS cookies.

### ☁️ Serverless edge (WebAssembly)

Prebuilt bundles for six edge platforms live on the
[**`deploy` branch**](https://github.com/LeenHawk/gproxy/tree/deploy), so you do
not need a Rust toolchain to deploy them. Edge deployments use **Turso** for
persistent configuration and can optionally use **Upstash** for shared caching.
See the [edge deployment guide](https://gproxy.leenhawk.com/deployment/edge/)
for platform-specific setup.

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https://github.com/LeenHawk/gproxy&branch=deploy&create_from_path=netlify)

The Cloudflare and Netlify buttons prompt for the required `TURSO_URL` and
`TURSO_TOKEN` secrets before deployment. Use Turso's HTTP URL
(`https://<db>.turso.io`), not the `libsql://` URL. Optional Upstash cache and
`GPROXY_MASTER_KEY` secrets can be added after the worker/site is created.
Cloudflare Workers, Netlify Edge, Deno Deploy, and EdgeOne Pages also ship the
Console assets in the same deployment. Set `GPROXY_ADMIN_USER` and a non-blank
`GPROXY_ADMIN_PASSWORD`, then open `/console` after deploy.

| Platform | Bundle | Deploy |
|---|---|---|
| Cloudflare Workers | [`deploy/cloudflare`](https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare) | Deploy button or `wrangler deploy` |
| Netlify Edge | [`deploy/netlify`](https://github.com/LeenHawk/gproxy/tree/deploy/netlify) | Deploy button or `netlify deploy --prod` |
| Deno Deploy | — | `deploy/deno/build.sh` (CLI) |
| Supabase Edge | [`deploy/supabase`](https://github.com/LeenHawk/gproxy/tree/deploy/supabase) | `supabase functions deploy gproxy` (Docker/eszip, CLI) |
| EdgeOne Pages | [`deploy/eopages`](https://github.com/LeenHawk/gproxy/tree/deploy/eopages) | `edgeone pages deploy` (CLI) |
| **Appwrite Functions** | [`deploy/appwrite-deno`](https://github.com/LeenHawk/gproxy/tree/deploy/appwrite-deno) | `appwrite push functions` (deno-2.0, CLI) |

### 📦 Native installers and binaries

Start with the **[download page](https://gproxy.leenhawk.com/getting-started/downloads/)**
or the [latest GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest).
Releases include Android APK, Windows MSI, macOS DMG, Linux DEB, and portable
ZIP builds. Linux GNU and musl builds support x86_64, AArch64, and RISC-V 64;
both Docker image families publish the same three architectures. Installed
desktop builds run in the background and expose an automatic-start switch in
Console Settings.

> If you only want to use GPROXY, download a prebuilt package. Do not clone and
> compile the repository unless you are developing GPROXY or need a custom build.

---

## Configure

Environment variables configure the process itself. Providers, credentials,
routes, users, and other live settings are stored in the database and managed
through `/console`.

| Variable | Default | Purpose |
|---|---|---|
| `GPROXY_HOST` / `GPROXY_PORT` | `127.0.0.1` / `8787` | bind address |
| `GPROXY_PERSISTENCE` | binary: `db`; Docker: `file` | `db` uses SQLite/PostgreSQL/MySQL; `file` stores JSON files and is single-instance only |
| `GPROXY_DSN` | generated SQLite DSN | Optional PostgreSQL/MySQL/SQLite DSN when `persistence=db` |
| `GPROXY_MASTER_KEY` | — | unseal stored secrets (absent = plaintext) |
| `GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` | `admin` / random | first-boot admin |

**Upgrading from v1?** Point v2 at the existing SQLite database. On first boot,
GPROXY imports the supported configuration and keeps the old database as a
`*.v1.bak` backup.

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

- **[Downloads](https://gproxy.leenhawk.com/getting-started/downloads/)**
- **[Documentation home](https://gproxy.leenhawk.com/)**
- **[Quick start](https://gproxy.leenhawk.com/getting-started/quick-start/)**
- **[Prompt caching](https://gproxy.leenhawk.com/guides/claude-caching/)**
- **[Edge deployment](https://gproxy.leenhawk.com/deployment/edge/)**
- **[Adding a channel](https://gproxy.leenhawk.com/guides/adding-a-channel/)**

## Star History

<a href="https://www.star-history.com/?repos=LeenHawk%2Fgproxy&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=LeenHawk/gproxy&type=date&theme=dark&legend=top-left&sealed_token=iBMEerxT7ZQBXRPAHN9XTVM7w_MUgcZCBVAwDDknpHwlPYhZueJ3_ZWhMXa7g67GF9AB9bzaqgBLVC9t5mrlxDZp3sqV-WwLo_JEx5fSsXDYfydUue3XsJlf1ScEWqGCVNW7TnR561_ETJnwEd4Xj61R4S9K5u_DvAD3aYkrxDikk_YkjB-HMUzAs5FG" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=LeenHawk/gproxy&type=date&legend=top-left&sealed_token=iBMEerxT7ZQBXRPAHN9XTVM7w_MUgcZCBVAwDDknpHwlPYhZueJ3_ZWhMXa7g67GF9AB9bzaqgBLVC9t5mrlxDZp3sqV-WwLo_JEx5fSsXDYfydUue3XsJlf1ScEWqGCVNW7TnR561_ETJnwEd4Xj61R4S9K5u_DvAD3aYkrxDikk_YkjB-HMUzAs5FG" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=LeenHawk/gproxy&type=date&legend=top-left&sealed_token=iBMEerxT7ZQBXRPAHN9XTVM7w_MUgcZCBVAwDDknpHwlPYhZueJ3_ZWhMXa7g67GF9AB9bzaqgBLVC9t5mrlxDZp3sqV-WwLo_JEx5fSsXDYfydUue3XsJlf1ScEWqGCVNW7TnR561_ETJnwEd4Xj61R4S9K5u_DvAD3aYkrxDikk_YkjB-HMUzAs5FG" />
 </picture>
</a>

## Support

If GPROXY is useful to you, you can support its continued development through
[GitHub Sponsors](https://github.com/sponsors/LeenHawk).

## License

[AGPL-3.0-or-later](LICENSE) · Author: [LeenHawk](https://github.com/LeenHawk)
