---
title: "CLI Clients"
description: "Point Codex CLI and Claude Code at GPROXY; what the emulated vendor control planes serve locally, what they forward, and their limits"
---

Some vendor CLIs talk to more than an inference endpoint: they fetch account
profiles, usage, plugins, or files from the vendor's control plane. A channel
can declare a **service surface**: a table of those control-plane paths and,
for each, whether GPROXY answers locally, forwards it on one pinned
credential, or treats it as an alias of an ordinary operation. Every entry
passes the same authentication, permission, quota, and settlement path as a
normal request, so a CLI is metered like any other client.

## Named Prefixes

Control-plane paths are vendor-specific, so they are reached through a named
prefix: the first path segment names a provider, a route, or a model
namespace, and the rest is the vendor's native path.

```text
https://gproxy.example/codex/backend-api/codex/responses
                       ^^^^^ provider named "codex"
```

The portal's **Connect** card renders the snippets below with your origin,
your key, and the model you pick. When the model is namespaced
(`team-a/reviewer`), the base URL becomes `https://gproxy.example/team-a`.

## Codex CLI

The Codex snippet assumes a provider named `codex` (as created by
`GPROXY_BOOTSTRAP_CHANNELS=codex`), or a model namespace called `codex`.

```toml
# ~/.codex/config.toml
model = "gpt-5.4"
model_provider = "openai"
openai_base_url = "https://gproxy.example/codex/backend-api/codex"
chatgpt_base_url = "https://gproxy.example/codex/backend-api"
```

```sh
export CODEX_REFRESH_TOKEN_URL_OVERRIDE='https://gproxy.example/codex/oauth/token'
export CODEX_REVOKE_TOKEN_URL_OVERRIDE='https://gproxy.example/codex/oauth/revoke'
codex login --device-auth --experimental_issuer 'https://gproxy.example/codex' \
  --experimental_client-id app_EMoamEEZ73f0CkXaXp7hrann
```

### Login

GPROXY is the OAuth issuer for Codex. `codex login --device-auth` prints a
code and opens `<issuer>/codex/device`; you sign in to the portal, enter the
code, and approve. Codex then exchanges at `/codex/oauth/token`. GPROXY
issues its own access token (valid one hour) and refresh token (30 days),
creates a key labelled `Codex OAuth` for the portal user, and maps every
request carrying the token to that key. Refresh and revoke go to the override
URLs above. The browser flow (`/codex/oauth/authorize`, callback on
`localhost:1455` or `1457`) works the same way.

### What Is Served

| Paths | Handling |
| --- | --- |
| `responses`, `responses/compact`, `images/generations`, `images/edits`, `alpha/search`, `realtime/calls`, `memories/trace_summarize`, `guardian`, `guardian-classifier` under `/backend-api/codex`, `/backend-api/wham`, `/backend-api`, `/api/codex`, `/codex` | Aliases of the ordinary operations: routing, transforms, failover, settlement. |
| `usage`, `usage/thread_usage/query`, `accounts/check`, `profiles/me`, `settings/user`, `workspace-messages`, `config/bundle`, `rate-limit-reset-credits`, analytics, `whoami`, workspace settings | Answered locally. Usage reports GPROXY's settled usage for the caller plus the credential's observed 5-hour and 7-day windows. |
| `models`, `agent-identities`, `mcp`, plugins, connectors directory, `environments`, `tasks`, `files` | Forwarded on one credential. MCP sessions pin by `mcp-session-id`; tasks and files remember the credential that created them. |
| `remote/control/...` | WebSocket bridge to the vendor on the pinned credential. |

Provider settings JSON can shape the local answers: `codex_pat_plan_type`
(`free`, `go`, `plus`, `pro`, `team`, `business`, `enterprise`, `edu`;
default `pro`), `codex_virtual_settings`, `codex_workspace_messages`,
`codex_config_bundle`, `codex_plugins_enabled`.

### Limits

Non-streaming Responses requests are converted to streaming upstream. Token
counting is answered locally. Embeddings are unsupported on the `codex`
channel. Thread-level usage is empty, and rate-limit reset credits report
none through the CLI; the console can consume them.

## Claude Code

```sh
export ANTHROPIC_BASE_URL='https://gproxy.example'
export CLAUDE_CODE_OAUTH_TOKEN='sk-gp-...'
claude --model 'claude-sonnet-4-6'
```

Claude Code sends the token as `Authorization: Bearer`, which admission reads
as a GPROXY key. The base URL is the plain origin: Messages, count tokens, and
model listing resolve by model name like any aggregated request, and the
control-plane paths are matched against every provider on the `claudecode`
channel.

| Paths | Handling |
| --- | --- |
| `/api/hello`, `/api/claude_cli/bootstrap`, `/api/claude_cli_profile`, `/api/claude_code_penguin_mode`, `/api/claude_code/skills`, `/api/oauth/organizations/{org}/skills/...` | Answered locally with synthetic account and organization ids derived from the provider and user. |
| `/api/oauth/file_upload`, `/v1/files`, `/v1/files/{id}`, `/v1/files/{id}/content` | Files API on one credential; uploads bind the file to the credential that stored it. |
| `/v1/skills`, `/v1/skills/{id}`, `/v1/skills/{id}/versions/...` | Skills API, bound the same way. |

Optional provider settings: `claudecode_bootstrap`, `claudecode_fast_mode`,
`claudecode_skill_health`, `claudecode_shared_skills` (JSON returned as-is).

Skill archive downloads return `404`. The account, organization, and plan
information Claude Code displays are GPROXY's synthetic values, not the
upstream account's.

## Session Affinity

Multi-turn CLIs work best when one conversation stays on one credential.
GPROXY derives a session subject from `x-gproxy-session-id` on any request;
from `session-id`, `x-session-id`, or `thread-id` on OpenAI-shaped requests;
and from `x-claude-code-session-id` or `session_id` on Claude Messages. The
pin lasts one hour of inactivity. With the `sticky` credential strategy the
same subject also selects the same credential.

## Gemini CLI

`geminicli` is an upstream channel: it pools Gemini CLI OAuth credentials and
serves Gemini, OpenAI, and Claude clients from them. GPROXY does not emulate
the Gemini CLI control plane, and the portal has no Gemini CLI snippet. Google
GenAI SDKs connect with the base URL set to your origin and the key in
`x-goog-api-key`; see [First Request](/getting-started/first-request/).
