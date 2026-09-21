---
title: CLI Clients
description: "Point the Codex CLI and Claude Code at GPROXY: the vendor service routes a channel declares, the three views, and the OAuth issuer this instance runs."
---

Some vendor CLIs talk to more than an inference endpoint. They fetch account
profiles, usage windows, plugins, tasks and files from the vendor's control
plane, and they refuse to work without them.

A channel can therefore declare a **service route table**: the control-plane
paths it knows, and what each one returns. The host matches an incoming path
against that table before it tries the data plane.

## Reaching Them

A control-plane path is vendor-specific, so it is only matched on a **provider
or namespace mount**. Matching a service route needs a channel, and the
aggregated mount names none.

```text
https://gproxy.example/codex/backend-api/wham/profiles/me
                       ^^^^^ the provider (or namespace) named "codex"
```

## The Three Views

`x-gproxy-view` names what the caller wants back. Absent means `caller`, which
is the only view an ordinary member may ask for.

| Header | What it answers |
| --- | --- |
| `x-gproxy-view: caller` | **This caller's** facts — their usage windows, their budget chain, the resources their own calls created. The vendor is never reached for identity, usage or settings. |
| `x-gproxy-view: pool` | The same, aggregated over every credential in the target. |
| `x-gproxy-view: credential:{id}` | Raw. The named credential's own auth goes upstream and the vendor's answer comes back untouched. |

Who may ask for which is decided by ownership, not by a flag. The target is the
caller's **visible** credentials of that provider — the same set a model call
from the same key could spend — and the caller is an administrator of it only
when they are an instance administrator, or an `Admin` member of the
organization or team that owns **every** credential in that set. Anything else
is a member, and a member may only ask for `caller`; `pool` and
`credential:{id}` are `403`.

"Every credential" is not a formality: administering one organization must not
render a pool that also contains another's. An unowned credential has no
administrator by construction — nobody is an admin member of nothing — so on a
single-tenant instance where every credential is shared, only an instance
administrator reaches `pool` and `credential`.

**Key minting is refused under every view.** A long-lived API key on a shared
subscription is exactly the credential a pooled gateway exists to avoid handing
out.

A path the table does not know is `404`, unless the view is `credential:{id}`.

## What Is Not Metered

A vendor service call takes **no rate-limit lease** — charging a window for a
profile fetch would spend an allowance the caller needs for traffic that costs
money — and writes **no capture record**. The engine writes no upstream record
for a service, so a downstream one would have nothing to link to and would read
as a request that reached no upstream.

It does pass the budget chain, because the `caller` view renders the caller's
own windows from it. Reporting a quota is not spending one.

A service call also cannot be cancelled: neither the host's request shape nor
the engine's has a field to put a token in, and a service call is short and
buffered enough that the value has not paid for changing two crates.

## Codex CLI

The `codex` channel declares the ChatGPT backend calls the CLI makes besides
Responses. Wire facts follow the CLI's own source under `samples/codex`:

| Family | Paths |
| --- | --- |
| plugins and MCP | `/backend-api/ps/**` |
| account, settings, usage, tasks, environments, remote control | `/backend-api/wham/**` |
| uploads | `/backend-api/files/**` |
| workspace plugin sharing | `/backend-api/public/plugins/**` |
| the start-up identity probe | `GET /v1/user-auth-credential/whoami` |

The CLI addresses the same endpoints two ways — `/backend-api/wham/**` on
ChatGPT hosts and `/api/codex/**` on Codex API hosts. Both spellings, and the
bare `/codex/**` and `/ps/**` mounts, are normalized onto `/backend-api/**`
before the table is consulted, so a client's choice of host style is not a
configuration question.

`/backend-api/wham/remote/control/server` is a **WebSocket**, and it is the one
service route that upgrades. Codex refuses a synthesized view outright, so
`x-gproxy-view: credential:{id}` from an administrator of that credential is
the only way through.

Anything the table does not classify falls into the `/backend-api/{*path}`
prefix rows and is forwarded as-is, non-2xx answers included.

## Claude Code

The `claudecode` channel declares the OAuth-scoped `/api/**` calls the CLI
makes besides Messages: profile, validate, roles, bootstrap, usage, policy
limits, account settings, file upload and download, organization connectors,
plugins and skills, onboarding and billing controls, desktop update redirects
and the Claude Design surface.

All of them resolve against `api.anthropic.com`; `claude.ai` serves only the
cookie login, so the channel does not consult it here.

Under `caller` and `pool` the identity, usage and settings answers are
synthesized from what this instance knows, and their ids are a stable hash of
the provider and the caller identity. Nothing about the shared account is
invented and nothing about it leaks: the account, organization and plan the CLI
displays are GPROXY's synthetic values, not the upstream account's.

## GPROXY as the OAuth Issuer

A CLI that logs in with OAuth can log in to **this instance** instead of to the
vendor. That is a different thing from the sdk's `login()`, which speaks
somebody else's OAuth as a client:

```text
  Claude Code ──authorize/token──▶ gproxy   (the authorization server)
                                     │
                                     └──login()──▶ OpenAI   (the client)
```

The endpoints sit under each mount, so a client discovers the one it is talking
to:

```text
GET  {mount}/v1/oauth/authorize
POST {mount}/v1/oauth/authorize
POST {mount}/v1/oauth/token
POST {mount}/v1/oauth/device/code
POST {mount}/v1/oauth/revoke
GET  {mount}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{mount}/v1
```

```sh
curl -s http://127.0.0.1:7070/v1/.well-known/oauth-authorization-server
```

```json
{"issuer":"http://127.0.0.1:7070/v1",
 "authorization_endpoint":"http://127.0.0.1:7070/v1/oauth/authorize",
 "token_endpoint":"http://127.0.0.1:7070/v1/oauth/token",
 "device_authorization_endpoint":"http://127.0.0.1:7070/v1/oauth/device/code",
 "revocation_endpoint":"http://127.0.0.1:7070/v1/oauth/revoke",
 "response_types_supported":["code"],
 "grant_types_supported":["authorization_code","refresh_token",
   "urn:ietf:params:oauth:grant-type:device_code"],
 "code_challenge_methods_supported":["S256"]}
```

v3 served these at the root; v4 moved them under `/v1` so the prefix rule is
the same one `/v1/messages` follows and there is one implementation of it.

### Rules this issuer does not bend

- **PKCE S256, always.** `plain` is refused and so is an absent challenge.
  Every registered client is public, with no secret to prove with, so the
  verifier is the only thing standing between a leaked code and a token.
- **Redirect URIs match exactly.** Not a prefix, not a wildcard, not "same
  origin". Every looser rule accepts a URL an attacker controls.
- **Codes, refresh tokens and device codes are single-use**, consumed and
  replaced in one atomic batch, so two redemptions cannot both succeed however
  they interleave.
- **A replayed code or refresh token revokes the whole family** — the grant,
  its internal key and every token it ever issued. There is no way to tell a
  lost response from a stolen credential, so the leak is assumed. The
  legitimate client re-runs its login; the thief cannot. The device flow is the
  exception, because a polling client re-sends its code by design.

### What an OAuth caller may do

An access token is a credential a user handed to **somebody else's binary**.
Unless the client is named in the instance's `oauth.cliClientIds`, it may only
list models, get a model, count tokens, generate, stream and compact. Anything
else is `403`, and that limit applies to an instance administrator's account
too — an administrator's is exactly the account where a third-party token must
not become an administrative credential.

Naming a client in `cliClientIds` is an operator accepting that it speaks for
the user across the whole API.

An OAuth caller also may not mint, rotate, reveal or delete keys, and may not
change the account password.

## Session Affinity

A multi-turn CLI works best when one conversation stays on one credential.
GPROXY reads the conversation identity from the client that **actually sent**
the request, never from the upstream it is about to be forwarded to — a Claude
Code request forwarded to another channel still reads Claude Code's field.

The ladder, first non-empty wins:

1. `x-gproxy-session-id`, the gateway's own header;
2. `thread-id`, `session-id`, `x-claude-code-session-id`, `x-conversation-id`,
   `x-grok-session-id`;
3. the inbound body's own field — Responses'
   `client_metadata.thread_id`/`session_id`, Claude's `session_id` *inside* the
   JSON-encoded `metadata.user_id`, Gemini's `request.session_id` and
   `request.sessionId`;
4. a sha256 fingerprint of the conversation's stable prefix;
5. the request id, labelled honestly as a fallback rather than claimed as a
   session.

Nothing recurses through arbitrary JSON looking for a field called
`session_id`, and request, turn and cache identifiers — `x-grok-req-id`,
`user_prompt_id`, `prompt_cache_key`, `previous_response_id` — are **never**
sessions.

The gateway header is stripped before the request goes upstream, and it is
stripped from a client's request too: a client must not be able to choose
another caller's conversation by sending the header the gateway uses to name
one. A missing session header never refuses a request.
