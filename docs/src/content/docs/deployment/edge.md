---
title: "Edge deployment (Cloudflare Workers)"
description: "Configure Workers, D1, secrets, and console assets, and review current deployment limitations."
---

Workers uses the same HTTP routes as the native server, with remote database and file-storage backends. The console is deployed as Workers Assets rather than embedded in WASM.

## One-click deployment

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/dev/deploy/cloudflare-button)

The template downloads and verifies the official v4.0.1 Worker and console bundle. No Rust toolchain is needed.

1. Click the button and connect your GitHub or GitLab account.
2. Choose the Worker and D1 database names. Cloudflare provisions and binds D1.
3. Set `GPROXY_ADMIN_PASSWORD` (at least 8 characters) and `GPROXY_MASTER_KEY` (a saved 32-byte key, for example the output of `openssl rand -hex 32`).
4. Keep the detected build command `npm run build` and deploy command `npm run deploy`.
5. Open `/console/` after deployment and sign in as `admin` with your password. Add providers and create a gateway API key.

The template pins its version in `prepare-release.mjs`. Keep D1 and the master key when updating. On startup, `GPROXY_ADMIN_PASSWORD` overrides the password of the administrator named by `GPROXY_ADMIN_USER` (default `admin`). An unchanged password preserves existing sessions; a changed password ends them.

## Current limitations

- Responses WebSocket, Realtime and channel service sockets share the native deployment’s routes, authentication and limits.
- D1 is enabled by default. libSQL and S3/R2 require their corresponding build features.
- Local SQLite, TCP databases, filesystem storage, and in-memory cache are unsupported. Current Worker assembly uses `store` cache and does not provide a Redis client.
- An empty database gets an administrator from `GPROXY_ADMIN_PASSWORD`. On later starts, the secret overrides that administrator’s password. The username defaults to `admin`; set `GPROXY_ADMIN_USER` to select it. Workers does not show the Application wizard.

## Use a release bundle

Download `gproxy-edge-cloudflare.zip` from the selected release, extract it, and enter the `cloudflare` directory. It includes the built Worker, console assets, and `wrangler.toml`.

Install dependencies and create a D1 database:

```sh
pnpm install
pnpm exec wrangler d1 create gproxy
```

Copy the returned database ID into `wrangler.toml`, keeping the binding name `DB`.

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "replace-with-your-database-id"
```

Save a 32-byte master key (64 hex digits or base64), then enter it as a secret:

```sh
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler secret put GPROXY_ADMIN_PASSWORD
pnpm exec wrangler deploy --dry-run
pnpm exec wrangler deploy
```

Keep the master key and reuse it on subsequent deployments. Without it, credentials are stored unencrypted.

Use the dry run to check the current bundle size against platform limits. Old WASM size measurements do not establish whether a new build fits.

## Configuration

`GPROXY_CONFIG` is a JSON document; named secrets override the corresponding fields. Defaults use the `DB` D1 binding and database-backed cache.

```toml
[vars]
GPROXY_CONFIG = """
{
  "store": { "kind": "d1", "binding": "DB" },
  "cache": { "kind": "store" },
  "public_base_url": "https://gproxy.example.workers.dev"
}
"""
```

| Secret | Purpose |
| --- | --- |
| `GPROXY_ADMIN_PASSWORD` | Administrator password, at least 8 characters; overrides the named administrator’s password on startup |
| `GPROXY_MASTER_KEY` | Master key for credential encryption |
| `GPROXY_LIBSQL_TOKEN` | libSQL / Turso token, when that backend is enabled |
| `GPROXY_S3_ACCESS_KEY_ID` | S3 / R2 access identifier |
| `GPROXY_S3_SECRET_ACCESS_KEY` | S3 / R2 secret key |

For published files or vocabularies, configure S3/R2 file storage and enable `s3`. Set `public_base_url` when upstreams need public file links.

## Console and routing

The bundle includes the following Assets configuration. Non-console requests go to the Worker first, including configurable provider prefixes.

```toml
[assets]
directory = "./public"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/*", "!/", "!/console", "!/console/*"]
```

After the first request initializes the instance, sign in at `/console/` with `admin` and the configured password, then create a gateway API key. For an existing database, a supplied secret overrides the named administrator’s password when a Worker isolate initializes. Removing the secret keeps the stored password. Invalid passwords or a missing named administrator fail startup; a new database also requires the secret. `/healthz` is a health check, not a substitute for authentication and upstream-request validation.

## Database and configuration synchronization

Each isolate synchronizes the Store schema during first assembly, before loading configuration and identity. The current package does not depend on separate Wrangler SQL migration files; `wrangler d1 migrations apply` is not the GPROXY initialization step.

Before each request, the instance checks the configuration revision and refreshes loaded state. Other isolates observe configuration writes on subsequent requests. Back up the database before upgrading and inspect first-request logs.

## Build from source

From the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

Validate the database, authentication, and at least one upstream call on Cloudflare before relying on the deployment. Compilation and a Wrangler dry run do not establish live functionality.
