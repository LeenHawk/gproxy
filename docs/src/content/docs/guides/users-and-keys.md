---
title: "Users & API Keys"
description: "Organizations, teams, users, and API keys; the administrator account, the user portal, admin API access, and how a key is sent"
---

Gateway traffic authenticates as a user through a user API key. The console
and the portal authenticate with a username and password and a server-side
session.

```text
Organization
`-- Team
    `-- User
        |-- password    optional; console or portal login
        |-- is_admin    grants /admin and the admin API
        `-- API keys    gateway traffic
```

A user may belong to a team, to an organization directly, or to neither.
Every level has an enabled flag. Permissions, rate limits, and quotas attach
at any level and are inherited downward; see
[Permissions, Rate Limits & Quotas](/guides/permissions/).

## The Administrator

The administrator is an ordinary user with a password and `is_admin`. On a
fresh store the console shows a setup page that creates it. Alternatively seed
it from the environment:

```sh
GPROXY_ADMIN_USER=admin            # default
GPROXY_ADMIN_PASSWORD=<password>   # required for the bootstrap options below
GPROXY_BOOTSTRAP_ADMIN_API_KEY=sk-...   # optional; a sealed key is generated otherwise
GPROXY_BOOTSTRAP_CHANNELS=codex,claudecode   # optional; creates one provider per id, named after it
```

Existing accounts are never changed by these variables except the
administrator's password, which is applied on every start when set.

Admin sessions use the `gproxy_admin_session` cookie, valid for 12 hours,
scoped to `/admin`. The admin API under `/admin/api/` also accepts
`Authorization: Bearer <key>` where the key belongs to an enabled user with
`is_admin`; there is no separate admin key type. Every admin write and every
secret reveal is recorded in the admin action audit.

## Creating Users

In **Identity**, create an organization, optionally a team inside it, then a
user. A user has a name, an optional organization and team, an enabled flag,
the administrator role, and an optional password. Leave the password blank on
edit to keep the current one.

## Issuing Keys

A key is created for one user with:

| Field | Meaning |
| --- | --- |
| Prefix | `sk` (standard) or `at` (Codex-style access token). The digest ignores the prefix, so both spellings identify the same key. |
| Label | Optional. |
| Expires at | Optional. Must be in the future; expired keys are rejected. |
| Enabled | Disabled keys are rejected. |

The full key has the shape `sk-gp-<43 url-safe characters>`. It is shown
once at creation. Lists show the first 12 characters. **Reveal key** returns
the full key again for keys whose sealed material is stored, and is an
audited action (`user_key.reveal`). Keys imported by digest only cannot be
revealed.

Keys are looked up by digest. The digest algorithm carries a version so it
can change without invalidating stored keys; version 1 is SHA-256 of the key
payload. Keys stored with a version this binary does not support are ignored.

There is no in-place rotation: create a new key, move clients, then disable
or delete the old one.

## Sending the Key

Admission reads the key from the first header present:

| Header | Typical client |
| --- | --- |
| `Authorization: Bearer <key>` | OpenAI SDKs, Codex CLI, Claude Code |
| `x-api-key: <key>` | Anthropic SDKs |
| `x-goog-api-key: <key>` | Google GenAI SDKs |

The Gemini `?key=` query parameter is not accepted; set the header. Any
header works on any path: the header only carries the key, it does not select
a protocol.

## The User Portal

Non-admin users sign in at `/portal` with the username and password an
administrator set. The session cookie `gproxy_portal_session` lasts 12 hours.
Login attempts are rate-limited per source address and per username.

The portal shows:

- **Connect**: the base URL, the models the account may call, and copy-paste
  snippets for curl, the OpenAI, Anthropic, and Google SDKs, Codex CLI, and
  Claude Code (see [CLI Clients](/guides/cli-clients/));
- **Quota windows** with the spending bars that apply to the account;
- **Usage and cost** over 1, 7, or 30 days;
- **Recent settled requests**, when the administrator enables it in the
  portal settings (bodies are never shown);
- **API keys**: create with `sk` or `at` prefix, revoke; and a password
  change form.

Portal keys cannot be revealed after creation.

## OAuth-Issued Keys

When Codex CLI signs in through GPROXY's built-in OAuth issuer, the approving
portal user receives a key labelled `Codex OAuth` with an `at-gp-oauth-`
prefix, and the access tokens GPROXY issues map back to it. Those requests are
metered against that user like any other. See
[CLI Clients](/guides/cli-clients/).
