---
title: Embedded Console
description: The built-in React console shipped inside the GPROXY binary.
---

GPROXY bundles a full **React console** into the release binary. When
the server starts, it serves the console at `/console` — there is no
separate frontend process to run or deploy.

## Accessing the console

1. Start GPROXY (see [Quick Start](/getting-started/quick-start/)).
2. Open `http://<host>:<port>/console` in a browser.
3. Log in with a username / password belonging to any enabled user. If
   the user is `is_admin = true`, you get the administrative views.

Login hits `POST /login`, which returns a **session token**. The UI
stores it and sends it on subsequent requests as
`Authorization: Bearer <session_token>`.

## What you can do from the console

- **Providers** — create, edit, disable; edit settings and credentials
  with channel-aware structured editors; watch per-credential health.
- **Models** — list models per provider, edit pricing, toggle enable
  state, define aliases, and use **Pull Models** to import the upstream's
  live model list.
- **Users** — create users, issue and revoke API keys, reset passwords,
  toggle admin status.
- **Permissions / Rate limits / Quotas** — per-user editors for the
  three access-control mechanisms (see
  [Permissions, Rate Limits & Quotas](/guides/permissions/)).
- **Observability** — usage dashboards, upstream / downstream request
  logs (when enabled), and health history.
- **Settings** — global proxy settings, logging toggles, self-update,
  and TOML rewrite-rule / sanitization editors.

## Rebuilding the console

The console source lives under `frontend/console/`. If you change it:

```bash
cd frontend/console
pnpm install
pnpm build
```

`pnpm build` writes assets into the path that the server crate embeds via
`include_dir!`. After rebuilding the frontend, rebuild the binary:

```bash
cargo build -p gproxy --release
```

There are no runtime static files to mount — the assets ship inside the
executable.

## Running the console in dev mode

For tight front-end iteration, you can run Vite's dev server against a
running backend:

```bash
cd frontend/console
pnpm install
pnpm dev
```

Set the Vite dev proxy to point at your local `gproxy` instance so
`/admin/*`, `/v1/*`, and `/login` round-trip against the real backend.

## Putting the console behind a reverse proxy

The console authenticates with a username/password and bearer token — it
does not integrate with external SSO on its own. If you need SSO, put
GPROXY behind a reverse proxy that terminates auth and forwards
requests. Restrict `/console` and `/admin/*` to authenticated sessions
at the proxy level, and continue to use the API-key flow for LLM
routes.
