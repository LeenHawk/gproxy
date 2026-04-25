---
title: Users & API Keys
description: How users, API keys, and admin accounts work in gproxy.
---

GPROXY is multi-tenant out of the box. Every request must authenticate as
a **user**, and every user carries one or more **API keys**.

## The data model

```text
User
├── name            (unique)
├── password        (Argon2 PHC; optional; used for the console login)
├── is_admin        (bool)
├── enabled         (bool)
└── keys[]
    ├── api_key     (secret; hashed + encrypted at rest when
    │                DATABASE_SECRET_KEY is set)
    ├── label       (free-form)
    └── enabled     (bool)
```

A user can exist without any keys (they still log into the console) and a
key can exist without a password (programmatic-only user). The `is_admin`
flag gates access to the `/admin/*` routes and to the administrative views
in the console.

## Creating users in the seed TOML

```toml
[[users]]
name = "alice"
password = "plain-or-argon2-phc"
enabled = true

[[users.keys]]
api_key = "sk-user-alice-1"
label = "default"
enabled = true

[[users.keys]]
api_key = "sk-user-alice-ci"
label = "ci-runner"
enabled = true
```

`password` accepts either plain text (which GPROXY will hash with Argon2 on
import) or a direct Argon2 PHC string (`$argon2id$…`), so you can bring
pre-hashed credentials in from an external system.

## The bootstrap admin

On startup, if the seed TOML does not define any user with `is_admin =
true` **and** at least one enabled key, gproxy will bootstrap an admin
account from these environment variables:

- `GPROXY_ADMIN_USER` (default `admin`)
- `GPROXY_ADMIN_PASSWORD` — if unset, a password is generated and logged
  once
- `GPROXY_ADMIN_API_KEY` — if unset, a key is generated and logged once

This is the "I just want to run the binary" path. Grab the logged values
the first time, paste them into your password manager, and you're in.

## Authentication surfaces

| Surface | Credential | Where |
| --- | --- | --- |
| LLM routes (`/v1/...`, `/v1beta/...`) | User API key | Depends on the protocol — `Authorization: Bearer …`, `x-api-key: …`, `x-goog-api-key: …`. |
| Console | Username + password | `POST /login` returns a bearer session token; the UI stores it and sends it as `Authorization: Bearer <session_token>`. |
| Admin API | Admin user API key | `Authorization: Bearer <admin api key>`. |

The console and admin API share the same router; the difference is
whether the authenticated user has `is_admin = true`.

## Managing users at runtime

Once the database is live, create and edit users from the console's
*Users* tab, or call the admin API directly. All admin endpoints are
**command-style**: `POST` with a JSON body. There is no REST-shaped
verb-on-path layer.

User CRUD:

- `POST /admin/users/query` — body `{}` or `{"id":{"eq":1}}` / `{"name":{"eq":"alice"}}`
- `POST /admin/users/upsert` — body is a `UserWrite` (insert if `id == 0`, update otherwise)
- `POST /admin/users/delete` — body `{"id": <user_id>}`
- `POST /admin/users/batch-upsert` — body is `[UserWrite, …]`
- `POST /admin/users/batch-delete` — body is `[<user_id>, …]`

User keys:

- `POST /admin/user-keys/query` — body `{}` or `{"user_id":{"eq":<user_id>}}`
- `POST /admin/user-keys/generate` — body `{"user_id": <user_id>, "label": "..."}`; the response carries the freshly generated `api_key` (plain text, shown only once)
- `POST /admin/user-keys/update-enabled` — body `{"id": <key_id>, "enabled": true|false}`
- `POST /admin/user-keys/delete` — body `{"id": <key_id>}`

Per-user quotas:

- `POST /admin/user-quotas/query` / `POST /admin/user-quotas/upsert`

All endpoints expect `Authorization: Bearer <admin api key>`.

Revoking a key takes effect immediately — the next request presenting it
will fail auth.

## At-rest encryption

When `DATABASE_SECRET_KEY` is set at startup, GPROXY enables the database
encryptor: user passwords and API keys (as well as provider credentials)
are encrypted with **XChaCha20-Poly1305** before being written to the
database. Losing the key means losing access to the ciphertext —
back it up out-of-band.
