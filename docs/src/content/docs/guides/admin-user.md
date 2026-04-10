---
title: Authentication and Authorization
description: Login flow, session tokens, API key extraction, admin vs user boundaries.
---

## Auth model overview

GPROXY has two credential types and two role levels:

- **Session tokens** -- for console/management access. Obtained via `/login`.
- **API keys** -- for proxy traffic. Managed per user via admin or user self-service API.

Role levels:

- **Admin** -- user with `is_admin = true`. Can access `/admin/*` routes.
- **User** -- any user. Can access `/user/*` routes with a session token. Can call provider proxy routes with an API key.

## Login

```
POST /login
Content-Type: application/json

{
  "username": "alice",
  "password": "secret"
}
```

Response:

```json
{
  "user_id": 1,
  "session_token": "sess-abc123...",
  "is_admin": false,
  "expires_in_secs": 86400
}
```

### Session tokens

- Prefixed with `sess-`.
- 24-hour TTL.
- Memory-only -- not persisted to the database. A server restart invalidates all sessions.
- Used for `/admin/*` and `/user/*` management routes.

Session tokens are intentionally separated from API keys so that a leaked inference key cannot be used to manage the account (create keys, view usage, etc.).

## API key extraction

For provider proxy routes, GPROXY extracts the API key from the request in this order:

1. `Authorization: Bearer <key>` header
2. `x-api-key: <key>` header
3. `x-goog-api-key: <key>` header
4. `?key=<key>` query parameter

The first non-empty value wins. If none are found, the request is rejected with 401.

## Route authorization

### `/admin/*` routes

Require one of:

- A session token (`sess-*`) belonging to an admin user.
- An API key owned by an admin user.

Non-admin tokens or keys get 403 Forbidden.

### `/user/*` routes

Require a session token (`sess-*`). API keys are not accepted for user management routes.

This is a deliberate security boundary: a leaked inference API key cannot be used to list other keys, create new keys, or query usage on the `/user/*` self-service routes.

### Provider proxy routes

Require a valid, enabled API key. Session tokens are not accepted. The key identifies the user for permission checks, rate limiting, and quota enforcement.

## Admin vs user boundary

| Capability | Admin (via session or admin key) | User (via session) | User (via API key) |
|------------|----------------------------------|--------------------|--------------------|
| Global settings | read/write | -- | -- |
| Provider management | create/update/delete | -- | -- |
| Credential management | create/update/delete | -- | -- |
| User management | create/update/delete all | -- | -- |
| Own API key management | -- | create/delete own | -- |
| Own usage query | -- | read own | -- |
| Provider proxy calls | -- | -- | yes |
| Model list/get | -- | -- | yes |
| Usage tracking | view all users | view own | tracked per request |
| Config export/import | yes | -- | -- |
| System update | yes | -- | -- |

### Recommendations

- Use admin keys for configuration and operations only, not for inference traffic.
- Issue dedicated user API keys per team, service, or environment for auditability.
- Enable logging flags (`enable_upstream_log`, `enable_downstream_log`) in global settings for audit trails.
- In production, keep `enable_upstream_log_body` and `enable_downstream_log_body` disabled unless actively debugging -- they log full request/response payloads.
