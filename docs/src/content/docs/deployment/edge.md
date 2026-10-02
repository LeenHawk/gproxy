---
title: "Hosted deployments"
description: "Deploy GPROXY on Cloudflare Workers, Netlify, Vercel, or Deno, connect a database, and sign in."
---

GPROXY provides deployment templates for Cloudflare Workers, Netlify, Vercel, and Deno. They download prebuilt release bundles, so no Rust installation is needed. The console and API share your deployment domain; the console is at `/console/`.

## Choose a platform

| Platform | Platform database | Existing database | WebSocket / Realtime |
| --- | --- | --- | --- |
| [Cloudflare Workers](#cloudflare-workers) | D1, created during deployment | libSQL / Turso | Supported |
| [Netlify Functions](#netlify) | Netlify Database | PostgreSQL | Not supported |
| [Vercel Functions](#vercel) | PostgreSQL product selected during deployment | PostgreSQL | Not supported |
| [Deno Deploy](#deno) | PostgreSQL attached after app creation | PostgreSQL | Not supported |

For HTTP and SSE streaming, you can use any template. For WebSocket or Realtime, choose Cloudflare Workers or a [CLI / container deployment](/getting-started/installation/).

Cloudflare runs WASM; the other three platforms run the native serverless executable. They share GPROXY's management API and console, while database types, function duration, and request size limits depend on the platform.

## Prepare the password and encryption key

Every platform needs these two settings:

| Setting | Value |
| --- | --- |
| `GPROXY_ADMIN_PASSWORD` | A login password of at least 8 characters |
| `GPROXY_MASTER_KEY` | A 32-byte key for encrypting upstream credentials, encoded as 64 hex characters or base64 |

You can generate a master key with `openssl rand -hex 32`. Save it and use the same value for updates and redeployments.

The username defaults to `admin`; set `GPROXY_ADMIN_USER` to choose another. The password setting is applied on startup: a same-name user's password is updated first; if no name matches, administrator `0` is recovered and given the configured name and password. See [administrator setup and password overrides](/reference/configuration/#bootstrap).

If you are using an existing database, you will also need its connection details. Each platform's steps are below.

## Cloudflare Workers

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)][cloudflare-auto]

[Use an external libSQL / Turso database][cloudflare-external]

Use the deploy button for D1, or the external-database link for an existing libSQL/Turso database.

1. Open your chosen entry and connect your GitHub or GitLab account.
2. For D1, choose a database name; Cloudflare creates and binds it. For an external database, enter `GPROXY_DATABASE_URL` and `GPROXY_LIBSQL_TOKEN`. That entry does not create D1.
3. Enter the administrator password and master key, keep `npm run build` and `npm run deploy`, and deploy.

A Turso URL can use `libsql://your-db.turso.io` or its HTTPS equivalent. This connection uses libSQL's HTTP interface, not a PostgreSQL connection string.

An existing D1 deployment can also switch to libSQL through these variables. `GPROXY_DATABASE_URL` takes precedence over D1. Changing the connection selects a different database; it does not migrate existing data.

For Wrangler deployment or custom Workers configuration, see [manual Cloudflare deployment](#manual-cloudflare-deployment) below.

## Netlify

[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)][netlify-auto]

[Use an existing PostgreSQL database][netlify-external]

Use the deploy button for Netlify Database, or the external-database link for an existing PostgreSQL database.

1. Open your chosen entry, authorize the repository, and enter the administrator password and master key.
2. For Netlify Database, the template reads the platform connection; no manual connection string is needed. For an existing database, enter its PostgreSQL connection string as `GPROXY_DATABASE_URL`.
3. Keep the template's build command and Functions configuration, then deploy.

An external connection looks like `postgresql://user:password@db.example.com/gproxy?sslmode=require`. Use the actual connection details supplied by your database service.

Both entries use the same Netlify template. Netlify may still provision a platform database when you supply an external connection; GPROXY gives `GPROXY_DATABASE_URL` precedence.

## Vercel

[![Deploy with Vercel](https://vercel.com/button)][vercel-auto]

[Use an existing PostgreSQL database][vercel-external]

Use the deploy button if you need a new database, or the external-database link for an existing PostgreSQL database.

1. Open your chosen entry. For a platform database, select and attach a PostgreSQL product when prompted. The external-database entry does not require a database product installation.
2. Enter the administrator password and master key. For an existing database, also set `GPROXY_DATABASE_URL`.
3. Keep the template's build command and `vercel.json`, then deploy.

Platform connections are read from `DATABASE_URL` or `POSTGRES_URL`. If your product uses another variable name, put its connection string in `GPROXY_DATABASE_URL`.

This template uses **Node.js Functions**. Do not switch it to Edge Runtime.

## Deno

[Create an application on Deno Deploy][deno]

Use the current Deno Deploy at `console.deno.com`. Select the `dev` branch containing the template and use `deno.json` from `deploy/serverless`.

1. Enter the administrator password and master key.
2. For an existing PostgreSQL database, set `GPROXY_DATABASE_URL`. For a platform database, create the app, then open **Databases** to create or attach PostgreSQL.
3. Redeploy after configuring the database. Platform bindings provide the connection through `DATABASE_URL`.

The Deno entry does not create a database automatically. The app may return 503 until the database is attached; complete the configuration and redeploy.

## Sign in and make a request

After deployment, open `https://your-domain/console/` and sign in with `admin` (or your chosen username) and the configured password.

Add a provider and credential in the console. Test the credential, create a model route and gateway API key, then follow [Your first request](/getting-started/first-request/). Clients use your deployment URL, such as `https://gateway.example.com/v1`, and authenticate with the gateway API key.

If Netlify, Vercel, or Deno returns 503, check the function logs and confirm that the database is attached, its connection string works, and the password and master key are set. The PostgreSQL account also needs permission to create the `gproxy` schema and application tables.

## Updates and runtime limits

Configuration, accounts, and usage live in the database. Keep the database and master key when updating. Each template pins its release in `prepare-release.mjs`, currently `v4.0.3`. To upgrade, change the version and redeploy; that release must include the matching platform bundles. Hosted deployments do not use the console's in-place binary updater.

The Netlify, Vercel, and Deno templates support HTTP and SSE, but reject WebSocket upgrades. Long-running inference, large files, and high concurrency remain subject to platform limits. Check the current [Netlify Functions](https://docs.netlify.com/build/functions/overview/), [Vercel Functions](https://vercel.com/docs/functions/limitations), and [Deno Deploy](https://docs.deno.com/deploy/reference/limits/) allowances for your workload.

These three templates do not configure file storage. For S3/R2 or publicly downloadable files, use a custom Workers build as described below, or the CLI. On Netlify, Vercel, and Deno, `GPROXY_PUBLIC_BASE_URL` can fix the public URL; otherwise it is derived from the request.

## Manual Cloudflare deployment

### Use a release bundle

Download `gproxy-edge-cloudflare.zip`, extract it, and enter the `cloudflare` directory. It contains the Worker, console, and `wrangler.toml`.

For D1, install dependencies and create the database:

```sh
pnpm install
pnpm exec wrangler d1 create gproxy
```

Put the returned database ID in `wrangler.toml`, keeping the binding name `DB`:

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "replace-with-your-database-id"
```

Set the master key and administrator password, then deploy:

```sh
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler secret put GPROXY_ADMIN_PASSWORD
pnpm exec wrangler deploy --dry-run
pnpm exec wrangler deploy
```

For libSQL, set `GPROXY_DATABASE_URL` and `GPROXY_LIBSQL_TOKEN` and remove the unused D1 binding. The release bundle enables libSQL; retain that feature when building your own. GPROXY creates its tables on first startup, so no separate Wrangler SQL migrations are needed.

### Workers configuration and static assets

`GPROXY_CONFIG` accepts a JSON configuration document. Named secrets override their corresponding fields. This example uses D1 and sets a public URL:

```toml
[vars]
GPROXY_CONFIG = '{"store":{"kind":"d1","binding":"DB"},"cache":{"kind":"store"},"public_base_url":"https://gateway.example.workers.dev"}'
```

Workers Assets serves the console. Keep the template's routing configuration so `/console/` uses static assets and other requests reach the gateway:

```toml
[assets]
directory = "./public"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/*", "!/", "!/console", "!/console/*"]
```

Workers supports D1 or libSQL, without local SQLite, TCP databases, or local file directories. For S3/R2, enable the `s3` feature at build time and set `GPROXY_S3_ACCESS_KEY_ID`, `GPROXY_S3_SECRET_ACCESS_KEY`, and the file-storage configuration. See the [configuration reference](/reference/configuration/) for the fields.

### Build from source

Run from the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

See [Building from source](/deployment/release-build/) for build and packaging options.

[cloudflare-auto]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-button
[cloudflare-external]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-external
[netlify-auto]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless
[netlify-external]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless#GPROXY_DATABASE_URL=
[vercel-auto]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY&products=%5B%7B%22type%22%3A%22integration%22%2C%22group%22%3A%22postgres%22%2C%22protocol%22%3A%22storage%22%7D%5D
[vercel-external]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY%2CGPROXY_DATABASE_URL
[deno]: https://console.deno.com/new?clone=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&path=deploy%2Fserverless
