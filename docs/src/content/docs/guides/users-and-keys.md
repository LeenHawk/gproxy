---
title: Users and API Keys
description: Manage organizations, teams, users, admin sessions, portal access, and user API keys in gproxy v2.
---

Gateway traffic authenticates as a user through a user API key. Console and
portal sessions authenticate with username/password and server-side sessions.

v2 uses a three-level identity hierarchy:

```text
Org
`-- Team
    `-- User
        |-- password       optional, for console / portal login
        |-- is_admin       grants /admin access
        `-- UserKey[]      API keys for gateway traffic
```

Users can be programmatic-only, interactive-only, or both.

## Identity Records

| Record | Purpose |
| --- | --- |
| `orgs` | Tenant boundary. An org can be disabled. |
| `teams` | Optional grouping inside an org. A user can belong to one team. |
| `users` | Login identity and API-key owner. |
| `user_keys` | Gateway API keys, indexed by digest in the control-plane snapshot. |

The hot path only indexes enabled users and enabled keys. Org and team rows are
also loaded so authorization can fail closed when a parent scope is disabled.

## Admin Users

`is_admin` gates `/admin/*`, `/healthz`, `/version`, and `/metrics`. Admin users
can manage providers, routes, users, rule sets, settings, usage, logs, and
updates from the console.

Non-admin users use the portal under `/user/*` for their own keys, limits,
security, audit, and usage views.

## API Keys

User keys are stored with:

- encrypted or sealed key material for display/export flows;
- a digest for hot-path lookup;
- a label;
- an enabled flag.

The console only shows the plaintext key at creation time. Runtime requests can
present the key through the credential shape of the inbound protocol, such as:

- `Authorization: Bearer <key>` for OpenAI-style clients;
- `x-api-key: <key>` for Claude-style clients;
- `x-goog-api-key: <key>` for Gemini-style clients.

The pipeline authenticates the key before route resolution and reads the matched
identity from `ControlPlaneSnapshot.keys_by_digest`, avoiding a persistence hit
on every request.

## Sessions and Cookies

Console login creates an httpOnly session cookie backed by the cache backend.
Local plain-HTTP development requires `GPROXY_INSECURE_COOKIES=1`. For
cross-site deployments, configure explicit credentialed CORS origins rather
than relying on wildcard CORS.

CSRF checks are same-origin by default. The Vite dev server proxies admin,
user, health, version, and metrics routes to the backend so same-origin cookies
work during local development.

## Secret Storage

`GPROXY_MASTER_KEY` enables secret sealing. With a master key, provider
credentials and user-key material are stored as envelopes. Without it, v2 runs
in plaintext compatibility mode and warns at startup/runtime. Existing plaintext
rows remain readable; sealed rows cannot be opened in plaintext mode.

Back up the master key. Losing it means sealed secrets cannot be recovered.
