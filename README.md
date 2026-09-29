# GPROXY

English | [简体中文](README.zh-CN.md) · [Documentation](https://gproxy.leenhawk.com/) · [Downloads](https://github.com/LeenHawk/gproxy/releases) · [Discussions](https://github.com/LeenHawk/gproxy/discussions)

**One endpoint for your LLM services.**

GPROXY is a self-hosted LLM API gateway. It manages upstream accounts, translates OpenAI, Claude, and Gemini requests, routes models, switches credentials on failure, enforces access rules, and records usage and costs.

This branch targets **4.0.0**, which is being prepared for release. To try v4 now, choose `nightly` on Releases. Once the stable version is published, choose its versioned attachments.

## Quick start

1. Download **Application** (`gproxy-tauri-*`) for your OS from [Releases](https://github.com/LeenHawk/gproxy/releases).
2. Open it and complete the three-step wizard: connection, administrator account, and optional configuration import.
3. Save the service URL and gateway API key. Open the console and add a provider and upstream credential.
4. Test the credential, create a model route named `fast`, and add the working model as a member.

```sh
export GPROXY_KEY='your-gproxy-api-key'
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Hello"}]}'
```

For a server, choose **CLI** (`gproxy-*`), extract it, and run:

```sh
./gproxy serve --data-dir ./data --port 8787
```

On Windows, use `gproxy.exe`. Save the administrator password and API key printed on first startup, then open **http://127.0.0.1:8787/console/**. Application uses its in-app console; its HTTP port serves gateway API calls only.

The CLI defaults to SQLite at `./data/gproxy.db`. Configure and save `GPROXY_MASTER_KEY` before adding credentials; without it, upstream secrets are stored unencrypted. Keep the data directory and master key when updating.

See [Installation](https://gproxy.leenhawk.com/getting-started/installation/) for platform requirements, signing restrictions, containers, and Workers. Windows MSIX attachments are unsigned Store submission packages, macOS apps are not notarized, and the HarmonyOS HAP is experimental.

## Features

- **Credential pools**: API keys, OAuth, or Cookies as supported by each channel, with health tracking, refresh, and failover.
- **Model routes**: stable client-facing names with configurable providers, upstream models, weights, and fallback tiers.
- **Protocol conversion**: major generation formats and streaming, with other operations depending on the channel and upstream.
- **Rewrite rules**: system text, cache breakpoints, JSON edits, regular expressions, and headers.
- **Users and costs**: users, organizations, teams, API keys, permissions, budgets, usage, requests, and audit records.
- **Deployment options**: CLI, desktop and mobile apps, containers, Cloudflare Workers, and an embeddable Rust core.

Read more: [Quick start](https://gproxy.leenhawk.com/getting-started/quick-start/) · [Providers and credentials](https://gproxy.leenhawk.com/guides/providers/) · [CLI clients](https://gproxy.leenhawk.com/guides/cli-clients/)

## Upgrade from v3

Stop v3, back up the database, master key, and startup configuration, then start v4 with the same configuration. Supported v3 SQLite, PostgreSQL, MySQL, and D1 databases migrate automatically, preserving accounts, passwords, API keys, and historical usage. The migration report lists configuration that could not be mapped. Do not run v3 and v4 against the same database at once.

See [Migrating v3 to v4](https://gproxy.leenhawk.com/deployment/v3-to-v4/) for migration scope, backups, and failure handling.

## Development

Native builds require stable Rust, Go, and Clang. The console and docs require Node.js 22.12+ (24 LTS recommended) and pnpm. Linux desktop builds also require the WebKitGTK 4.1, GTK 3, and libsoup 3 development packages.

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build
cargo run -p gproxy -- serve
```

The console build synchronizes embedded assets. A fresh checkout built with only `cargo build` has no console bundle.

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm --dir console lint
pnpm --dir console test
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs check
pnpm --dir docs build
```

See [Building from source](https://gproxy.leenhawk.com/deployment/release-build/) for desktop and WASM checks, [Architecture](https://gproxy.leenhawk.com/introduction/architecture/) for the workspace layout, and [Adding a channel](https://gproxy.leenhawk.com/guides/adding-a-channel/) for extensions.

Report bugs through [Issues](https://github.com/LeenHawk/gproxy/issues). Report vulnerabilities privately through [Security](https://github.com/LeenHawk/gproxy/security).

## License

The gateway application is **AGPL-3.0-or-later**; see [LICENSE](LICENSE). `gproxy-protocol`, `gproxy-protocol-macros`, `gproxy-client`, `gproxy-cache`, `gproxy-file`, `gproxy-seaorm`, and `gproxy-tokenizer` are **MIT**, with a license in each directory.
