# GPROXY

[English](README.md) | [简体中文](README.zh-CN.md) · [Documentation](https://gproxy.leenhawk.com) · [Downloads](https://github.com/LeenHawk/gproxy/releases) · [Discussions](https://github.com/LeenHawk/gproxy/discussions)

[![CI](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml/badge.svg)](https://github.com/LeenHawk/gproxy/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/LeenHawk/gproxy)](LICENSE)

**One endpoint for your LLM providers, accounts and applications.**

GPROXY is a self-hosted LLM API gateway. It pools upstream credentials, routes
requests, translates API formats, enforces access and spending limits, and
records usage and cost. A native installation serves the API, an operator
console and a user portal from one executable.

## What You Can Do

- **Use the client you prefer.** Accept OpenAI Chat Completions, OpenAI
  Responses, Claude Messages and Gemini GenerateContent, including streaming.
  Cross-format conversion is direct; support for other operations depends on
  the selected channel.
- **Pool upstream accounts.** Manage API keys, OAuth credentials and cookies
  where supported, with token refresh, health tracking, credential selection
  and failover.
- **Keep model names stable.** Map a public model name to a route and its
  provider/upstream models. Change providers without changing every client.
- **Control access and cost.** Manage users, organizations, teams, permissions,
  rate limits, spending quotas and dimensional pricing. Inference goes through
  the same admission and settlement pipeline, including CLI service surfaces.
- **Operate without editing JSON files.** The console manages providers,
  credentials, model catalogs, rule sets, usage, quota history and updates.
  The user portal manages accounts, API keys and authorized OAuth sessions.
- **Choose a deployment.** Native binaries and installers, containers, Android
  packages and prebuilt edge bundles are available. Rust applications can
  embed `gproxy-core` without an HTTP server or UI dependency.

Channels include OpenAI, Claude API / Claude Code / Claude Web, Gemini CLI,
Codex, Copilot, OpenRouter, AWS Bedrock, Vertex, Azure, Kimi and others.
See [Providers](https://gproxy.leenhawk.com/guides/providers/) for their
authentication methods and runtime-specific capabilities.

## Quick Start

### Native

Download the archive or installer for your platform from
[Releases](https://github.com/LeenHawk/gproxy/releases). After extracting a
portable archive on Linux or macOS:

```sh
chmod +x ./gproxy
./gproxy
```

On Windows, run `gproxy.exe`. Open **http://127.0.0.1:8787/admin** and create
the first administrator. The user portal is at **/portal**.

The default native installation listens on loopback and stores its database
at `./data/gproxy.db`. Keep that directory when updating the executable.

### Container

```sh
docker run -d --name gproxy --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v gproxy-data:/app/data \
  ghcr.io/leenhawk/gproxy:v3.0.0
```

The release image runs as UID/GID **65532:65532** and stores data at
**/app/data**. A named volume preserves it; bind mounts must be writable by
that user. Images cover amd64, arm64 and riscv64, with a `-musl` variant.
Use `:staging` only when you want rolling development builds.

### Edge

Cloudflare Workers and Netlify Edge run the prebuilt wasm bundle from the
[`deploy`](https://github.com/LeenHawk/gproxy/tree/deploy) branch. Both
buttons ask for `GPROXY_LIBSQL_URL` and `GPROXY_LIBSQL_AUTH_TOKEN`, the HTTPS
URL and token of a libSQL (Turso) database; everything else is optional.

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https://github.com/LeenHawk/gproxy&branch=deploy&create_from_path=netlify)

See [Container deployment](https://gproxy.leenhawk.com/deployment/docker/)
and [Edge deployment](https://gproxy.leenhawk.com/deployment/edge/) for other
deployment options.

### Send Your First Request

1. Add a provider in the console and supply or authorize an upstream credential.
2. Pull its model catalog, then configure the public model/route you want to use.
3. Grant your user access and create an API key.
4. Set `GPROXY_API_KEY` in your shell and replace `my-model` with an accessible
   model ID shown by the console or `GET /v1/models`.

```sh
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"my-model","messages":[{"role":"user","content":"Hello"}],"stream":true}'
```

Your application only needs the gateway base URL, a GPROXY API key and a model
ID. [First request](https://gproxy.leenhawk.com/getting-started/first-request/)
covers the other API formats.

## CLI Clients and Pi

Codex CLI and Claude Code can use GPROXY's compatible service surfaces.
Follow the [client guides](https://gproxy.leenhawk.com/guides/cli-clients/)
for the correct provider path and authentication mode.

For Pi, install the independent, MIT-licensed
[pi-gproxy](https://github.com/LeenHawk/pi-gproxy) extension:

```sh
pi install npm:pi-gproxy
```

Enable `pi-gproxy` in **Console → Settings → OAuth clients**, then run
`/login` in Pi and select **GPROXY**. Browser PKCE and device-code login
authorize your gateway account, not an upstream account. The extension
discovers your allowed models and uses the normal inference pipeline.

The Portal's **Authorized sessions** view shows successful logins, still-valid
sessions and refresh counts. Revoking a session invalidates its tokens.
Pi's local `/logout` alone does not revoke the server session.
See [Account OAuth](docs/account-oauth.md) for the contract.

## Configuration and Security

Configuration comes from command-line flags, environment variables, then
`.env` in the working directory and data directory. Run `gproxy --help` for
the complete native option list.

| Variable | Purpose |
| --- | --- |
| `GPROXY_HOST`, `GPROXY_PORT` | Listen address; native defaults to `127.0.0.1:8787`. |
| `GPROXY_DATA_DIR` | Persistent local state; native default `./data`, release container `/app/data`. |
| `GPROXY_PERSISTENCE` | `sqlite`, `libsql`, `postgres` or `mysql`. |
| `GPROXY_DSN` | Database connection string where required. |
| `GPROXY_MASTER_KEY` | Optional standard-base64 32-byte key for stored secret encryption. |
| `GPROXY_UPSTREAM_PROXY_URL` | Default upstream proxy override. |

Without a master key, stored credentials and API keys are plaintext. Protect
the data directory and backups. If you enable encryption, keep the key safe
and do not regenerate it on each restart; changing it requires the documented
rotation procedure. Put remote access behind HTTPS and configure trusted
proxies and allowed origins deliberately.

[Configuration reference](https://gproxy.leenhawk.com/reference/configuration/)
covers encryption, persistence, caching, bootstrap and proxy settings.

## Upgrading from v2

**Back up the v2 executable, database and launch configuration before updating.**

V3 uses a different data model. Native startup can back up and migrate supported
v2 SQLite databases, preserving recoverable keys and supported configuration
and usage. It verifies the migrated data before atomically switching databases.

Unmapped populated tables, route-specific permissions and other unsupported
data stop automatic migration rather than being silently discarded. Existing
v2 updaters do not retain the old executable, so binary rollback requires your
saved executable or the corresponding official v2 package. Remote databases
require an explicit migration plan.

Read the [upgrade and rollback guide](docs/v2-upgrade.md) first.
`main` now contains v3; v2 source remains available through its version tags
and Git history.

## Development

The Rust workspace separates the embeddable core, channel implementations,
pairwise transforms, shared persistence, application services and native/edge
hosts. The React console is in `console/`; documentation is in `docs/`.
Admin API TypeScript types are generated from Rust by `cargo test`.

```sh
cargo run -p gproxy-host-axum
pnpm --dir console install --frozen-lockfile
pnpm --dir console dev
```

Before submitting changes:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
pnpm --dir console lint
pnpm --dir console test
pnpm --dir console build
```

See [Architecture](https://gproxy.leenhawk.com/introduction/architecture/) and
[Adding a channel](https://gproxy.leenhawk.com/guides/adding-a-channel/).
Report bugs through [Issues](https://github.com/LeenHawk/gproxy/issues);
report vulnerabilities privately through
[Security](https://github.com/LeenHawk/gproxy/security).

## License

The gateway application is **AGPL-3.0-or-later**; see [LICENSE](LICENSE).
Some reusable protocol/transform crates use **MIT** as specified in their
individual `Cargo.toml` files. The separate Pi extension is MIT-licensed.
