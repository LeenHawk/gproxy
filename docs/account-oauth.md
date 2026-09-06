# Account OAuth

## Operators and users

OAuth authorizes a **GPROXY user account**, not an upstream account. One
authorization can discover and invoke models across channels, intersected with
the user's current permissions. It grants no administrator or Portal-management
access. Inference still uses the core admission, routing, settlement, capture and
telemetry path, including new turns on an established Responses WebSocket.

Console Settings contains the public-client registry: immutable `client_id`,
display name, redirect URLs and enabled state. No client secret is used. Names
and IDs identify a configured client; they do not prove the requesting program's
identity. Only approve a login you initiated.

`pi-gproxy` is seeded disabled, with `http://127.0.0.1/oauth/callback`. Enable it
before using the independent Pi package. A registered portless IP-loopback URL
allows a dynamic port; host, path and query must still match exactly. Other URLs
must match exactly; non-loopback HTTP, fragments and userinfo are rejected.
The existing Codex client and its localhost callbacks remain enabled.

Portal → Authorized sessions shows:

- **Total logins**: sessions whose initial code/device exchange issued tokens.
  Visiting, denying or abandoning consent does not count.
- **Still valid**: non-revoked sessions with a usable user, client and internal
  key, and an unexpired current refresh token. This is not an online-device count.
- **Successful refreshes**: committed token rotations for that same session.
  Failed, replayed or losing concurrent requests do not increase it.

Revocation retains history but invalidates all access/refresh tokens. Already
started responses finish and settle; subsequent requests/turns fail. Disabling
or deleting a client also invalidates pending authorizations. Explicitly
re-registering a deleted ID is allowed, but old grants remain revoked. Restart
does not restore deleted clients. Internal OAuth keys are excluded from ordinary
key lists, editing, plaintext export and API-key authentication.

## HTTP contract

All paths are shared by native and edge application hosts. Responses containing
authorization information are `no-store`; tokens are opaque for generic clients.

| Endpoint | Contract |
| --- | --- |
| `GET /oauth/authorize` | `response_type=code`, registered `client_id`, `redirect_uri`, `scope=gproxy`, `state`, PKCE S256; validates then redirects to Portal login/consent. |
| `GET /oauth/authorize/details` | Same query; authenticated Portal user; application/account display data. |
| `POST /oauth/authorize` | Same-origin Portal JSON decision, with `authorization` and `approved`; returns a callback URL preserving `state`. |
| `POST /oauth/token` | Form or JSON; `client_id` plus `authorization_code`, `refresh_token`, or `urn:ietf:params:oauth:grant-type:device_code` grant. |
| `POST /oauth/device/code` | Public `client_id`, optional `scope=gproxy`; returns `device_code`, `user_code`, verification URLs, expiry and polling interval. |
| `GET /oauth/device/details` | Portal consent details for `user_code`. |
| `POST /oauth/device/decision` | Same-origin Portal JSON `{user_code, approved}`. |
| `POST /oauth/device/cancel` | Client extension: `client_id` and secret `device_code`; invalidates the abandoned flow and any associated grant. |
| `POST /oauth/revoke` | `client_id` and `token`; revokes that client's complete session; unknown tokens are a successful no-op. |
| `GET /portal/api/oauth-sessions` | Cookie-authenticated, user-owned page: `active_only` (default true), `limit` (1–100), `offset`; history-wide summary. |
| `DELETE /portal/api/oauth-sessions/{id}` | Cookie-authenticated, same-origin, owner-only, idempotent revocation. |
| `/admin/api/oauth-clients[/{id}]` | Existing administrator authentication; list/create/update/delete. |

Token replies contain `token_type=Bearer`, `access_token`, `refresh_token` and
`expires_in`. Codex additionally receives its compatibility `id_token` and
ChatGPT claims. Named Codex URLs and legacy device envelopes remain adapters to
the same persistent issuer, not a parallel refresh implementation.

## Storage and authentication boundary

Schema version 4 introduces clients and session statistics, makes provider
association optional, and migrates existing Codex grants without rewriting
tokens. Login time and refresh history are derived from existing token issuance
records; absent evidence stays unknown. Back up production databases and validate
the migration on a copy before deployment.

Code consumption, token inserts and initial login recording commit together.
Refresh consumption, replacement tokens and counters also share one transaction.
Client/grant row locks serialize revocation against exchange; a consumed-by
receipt gates token inserts so competing refreshes cannot both succeed. A failed
write rolls back the entire rotation. SQL backends share schema and queries.

Authentication attaches the access-token digest to `CallerIdentity`. Core can
then remove bearer headers before forwarding without losing the authorization
binding. Admission validates that exact digest against current user/client/grant
state and refreshes user organization/team membership. WebSocket turns and
derived surface bindings preserve the digest and recheck it, including expiry.

OAuth DTOs are generated for Console by the existing `ts-rs` export test. No raw
access/refresh tokens are returned by the session-management endpoints.
