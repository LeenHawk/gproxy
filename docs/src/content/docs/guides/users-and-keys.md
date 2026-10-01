---
title: "Users and API keys"
description: "Create users, assign management scopes, and issue or rotate gateway API keys."
---


Use a username and password to sign in to the console, and a gateway API key to call models. Upstream credentials belong to providers and cannot sign you in to GPROXY.

## First administrator

Application creates the administrator in its first-run wizard. The CLI creates an administrator on an empty instance and prints a generated password and API key. You may supply initial values instead:

```sh
export GPROXY_ADMIN_USER='admin'
export GPROXY_ADMIN_PASSWORD='your-initial-password'
export GPROXY_BOOTSTRAP_ADMIN_API_KEY='your-initial-gateway-key'
./gproxy serve
```

On a new instance, the password and key are generated if omitted. **An explicit `GPROXY_ADMIN_PASSWORD` also updates a same-name user’s password, or recovers and renames administrator `0` if no name matches on restart; the API key option is first-run only.** `GPROXY_BOOTSTRAP_CHANNELS` no longer creates providers; add providers in the console.

CLI / container users sign in at `/console/`. Personal and management pages share the `gproxy_session` HTTP session cookie, whose lifetime is configured by `session_ttl_secs`. Application uses its in-app console.

## Users, organizations, and teams

Instance administrators create users under access control. A user's role is `admin` or `user`. A user without a password cannot sign in, but may use issued API keys with the appropriate permissions.

Organization and team memberships assign users and scoped management roles. An API key may be bound to an organization or team; this binding determines credential visibility and applicable budgets. Request headers cannot override the binding.

Organization and team administrators manage only the features available in their scope. They are not instance administrators. See [Console and scoped administration](/guides/console/).

## Create a key

Manage personal keys under **My account → Keys**. Instance administrators can also use **Access control → User keys**.

| Setting | Meaning |
| --- | --- |
| Name | Identify the app or device using the key |
| Ownership | User and optional organization or team |
| Expiry and enabled state | Expired or disabled keys cannot make new requests |
| Management | Permit management operations, subject to user and scope permissions; off by default |
| Retain secret | Allow the full value to be revealed later; off by default |
| Cost budget | Administrators can set an amount, period, and model filter |

Copy the full key when it is created. A key without retained secret material, including a digest-only import, cannot be revealed later. Revealing retained keys still requires permission.

An initial budget and key can be saved together. A failed budget validation does not create the key separately.

## Rotate or revoke

Keys support rotation. The old value becomes invalid, so update the client configuration. To migrate clients gradually, create a second key and disable or delete the old one after all clients have switched.

Management API operations:

- `POST /admin/api/api-keys/{id}/rotate`: rotate a key.
- `GET /admin/api/api-keys/{id}/secret`: reveal a retained secret.
- Personal API equivalents are `/portal/api/keys/{id}/rotate` and `/portal/api/keys/{id}/secret`.

## Send a key

```text
Authorization: Bearer <gateway-key>
x-api-key: <gateway-key>
x-goog-api-key: <gateway-key>
```

Use the header customary for the client protocol, without supplying conflicting keys. Management API access requires the key's management flag; an administrator's ordinary inference key does not automatically gain management access.

## OAuth sessions

GPROXY can issue access tokens after user authorization. OAuth internal keys differ from ordinary user keys and cannot be created or revealed as ordinary Bearer keys. Authorized sessions can be revoked in the console.

See [CLI clients](/guides/cli-clients/) for client configuration and authorization flows.
