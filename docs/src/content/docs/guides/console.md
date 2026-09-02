---
title: Console, Portal & Public Site
description: "The three web surfaces served by the gproxy binary, what each console section manages, and how the console is built and embedded"
---

The `gproxy` binary serves one React application at three paths. The build
is embedded into the binary, so there is nothing else to deploy.

| Path | Surface | API | Audience |
| --- | --- | --- | --- |
| `/` | Public site | none | anyone reaching the port |
| `/admin` and `/admin/*` | Operator console | `/admin/api/**` | administrators |
| `/portal` | User portal | `/portal/api/**` | users with a password |

Everything else on the port is gateway traffic authenticated by API key
(see [Routing & Endpoints](/reference/routing-table/)).

## First Boot

`GET /admin/api/session` reports `setup_required: true` until an
administrator exists. The console then shows **Create the administrator**;
`POST /admin/api/setup` accepts one username and password, creates the first
admin, opens a session and records an `auth.setup` audit event. The form is
rate limited to four attempts per minute per source address.

To skip the form, start the binary with `GPROXY_ADMIN_PASSWORD` (and
optionally `GPROXY_ADMIN_USER`, default `admin`). The account is created on
first run, an admin API key is generated or taken from
`GPROXY_BOOTSTRAP_ADMIN_API_KEY`, and `GPROXY_BOOTSTRAP_CHANNELS` can create
empty providers for the listed channel ids. The bootstrap key and channels
apply only on first run, but the named administrator's password is
reapplied on every start, so remove `GPROXY_ADMIN_PASSWORD` once you have
logged in. See [Configuration](/reference/configuration/).

## Signing In

The console signs in with `POST /admin/api/login` and holds a
`gproxy_admin_session` cookie: HttpOnly, `SameSite=Strict`, scoped to
`/admin`, valid for 12 hours. Mutating calls made with the cookie must be
same-origin. Scripts call the admin API with
`Authorization: Bearer <api-key>` where the key belongs to a user flagged
`is_admin`; bearer calls skip the same-origin check. Login and logout are
audited as `auth.login` and `auth.logout`.

## Console Sections

The sidebar lists ten sections. Paths are real URLs you can bookmark.

| Section | Path | What it manages |
| --- | --- | --- |
| Overview | `/admin` | Healthy-credential ratio, credentials needing attention, requests and settled cost over 24 h, hourly usage trend for 7 days, spending by provider, quota windows and upstream cycles at or above 80 %. |
| Providers | `/admin/providers` | Provider list and detail tabs: Credentials (pool, login wizard, health, quota cycles), Models (served models, variants, per-model pricing), Rules, Routing, Settings (channel fields, endpoint overrides, proxy, TLS fingerprint, forwarded metadata). |
| Load balancing | `/admin/routes` | Routes: name, maximum attempts, members (provider, pinned credential, upstream model, failover tier, weight), model mappings with exposed metadata, routing and model aliases. |
| Rules | `/admin/rules` | Rule sets, their mutation rules with effective order, and provider attachments. |
| Identity | `/admin/identity` | Organizations, teams, users and API keys; permissions, rate limits and quotas at each scope, with inherited values shown. |
| Statistics | `/admin/usage` | Tabs Usage, Admin actions (`/admin/audit`) and Request audit (`/admin/logs`). |
| Pricing | `/admin/pricing` | Price rules by model pattern, dimensional rates, context and service-tier ladders. |
| Tokenizers | `/admin/tokenizers` | Vocabulary switch, automatic fetching, default vocabulary, Hugging Face token, cached vocabularies (fetch with progress, delete). |
| Updates | `/admin/update` | Update channel and automatic-check preference, signed update check, apply, roll back, release notes. Native builds only. |
| Settings | `/admin/settings` | Instance settings, global metadata blacklist, retention and capture, configuration export and import, portal setting, login autostart. |

An update banner appears above every page when automatic checking is on and
a newer build exists on the selected channel. A native binary also shows the
signed announcement feed. The sidebar footer prints the build identity:
version, channel, short hash and installation kind.

## Instance Settings

`GET` and `PATCH /admin/api/instance-settings` carry these keys; the
Settings, Tokenizers and Updates pages edit subsets of the same record.

| Key | Meaning |
| --- | --- |
| `instance_name` | Label written into every usage row's dimensions. Default `default`. |
| `proxy` | Default upstream proxy URL, used after credential and provider overrides. |
| `inherit_system_proxy` | Honour `HTTP_PROXY` and `HTTPS_PROXY` when no explicit proxy applies. Off by default. |
| `enable_usage` | Persist usage rows after settlement. Default on; admission and billing still run when off. |
| `enable_tokenizer_vocabs`, `enable_tokenizer_download`, `default_tokenizer_vocab` | Count tokens with real vocabularies, fetch missing ones automatically, and the fallback vocabulary. |
| `file_upload_max_in_flight` | Concurrent file uploads; `0` is unlimited. `GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT` overrides it. |
| `retention_days`, `max_database_size_mb` | Observability cleanup bounds; at least one must be set before body capture can be enabled. |
| `enable_downstream_log`, `enable_downstream_log_body`, `enable_upstream_log`, `enable_upstream_log_body`, `disable_log_redaction` | Wire capture and redaction, see [Usage, Logs & Audit](/guides/observability/). |
| `update_channel`, `enable_auto_update_check` | `releases`, `staging` or `dev`; unset follows the channel the binary was built for. |
| `traffic_blacklist` | Extra request headers, response headers and query parameters stripped instance-wide before any channel allow-list. |

The portal's one setting, whether users may see recent settled requests,
lives in `GET` and `PATCH /admin/api/portal-settings`. **Test connectivity**
on the Settings page probes egress through the saved proxy chain and reports
the address the upstream would see.

## Theme and Language

Every surface offers English, 简体中文 and 繁體中文. The console and portal
also offer light, dark and system themes; the choice is stored in the
browser under `gproxy-console-theme`. The public site shows the language
menu only.

## Deep Links

| URL | Opens |
| --- | --- |
| `/admin/providers/<id>/<tab>` | A provider on `credentials`, `models`, `rules`, `routing` or `settings`; `/credentials/<credentialId>` opens one credential. |
| `/admin/routes/<id>/models`, `/admin/routes/<id>/settings`, `/admin/routes/new` | A route tab, or the create form. |
| `/admin/identity/<users\|teams\|organizations>/<id>` | An identity entity. `/admin/keys/...` is an alias. |
| `/admin/logs/<request_id>` | One captured request with its upstream attempts. |
| `/portal?oauth_return=/<path>` | After portal login, continue to a same-origin path; CLI sign-in flows use it. |

## Keyboard and Small Screens

Table rows and cards that open a detail are focusable and respond to Enter
and Space. Sidebar and workspace resize handles accept the arrow keys, Home
and End, and remember their width. Below the `lg` breakpoint the sidebar
becomes a horizontal, scrollable bar; below `md` the list-and-detail
workspaces show one pane at a time with a Back button, and data tables
render as cards.

## User Portal

Any user with a password can sign in at `/portal` (`POST /portal/api/login`;
cookie `gproxy_portal_session`, 12 hours). Administrators create users and
set their initial password under Identity; users change it in the portal.

| Panel | What it does |
| --- | --- |
| Account | Change the password. Create API keys with prefix `sk` (API clients) or `at` (Codex access-token login) and an optional label; the key is shown once. List and revoke own keys. |
| Connect | Pick an allowed model and copy a ready-to-run snippet: curl, OpenAI Python, Claude Python, Gemini Python, Codex CLI config, Claude Code environment. Snippets are limited to the wire formats the model can serve. |
| Allowed models | Live routes the account may call, with their capabilities. |
| Usage and cost | Settled requests, input, output and cached tokens, and cost over 1, 7 or 30 days. |
| Quota windows | Spending windows applied to the user, team and organization: total, daily, weekly, monthly, 5-hour and 7-day. |
| Recent settled requests | The latest 20 requests with provider, operation, upstream model, tokens, cost and latency. Shown only when the administrator enables it; bodies are never shown. |

Keys have the form `<prefix>-gp-<random>`. The Codex and Claude Code
snippets are explained in [CLI Clients](/guides/cli-clients/).

## Public Site

`/` is a landing page: a request-translation example that switches between
OpenAI Chat, Claude Messages and Gemini, the execution funnel, the product
claims, connection examples with a model placeholder, and links to the
admin console, the portal, the source repository and the licence.

## Building and Embedding the Console

The console lives in `console/` and is managed with pnpm.

```bash
cd console
pnpm install
pnpm build      # tsc -b, vite build, then scripts/sync-to-embed.mjs
```

The last step copies `console/dist/` into
`crates/gproxy-host-axum/assets/web/`, which `rust-embed` compiles into the
binary. Rebuild `gproxy` afterwards. A binary built without that directory
still serves the API; requests to `/` answer
`web assets are not embedded; run pnpm build in console/ and rebuild gproxy`.

For development, `pnpm dev` starts Vite and proxies the admin and portal
APIs to a running backend. Finish console changes with `pnpm lint` and
`pnpm test`. Types under `console/src/generated/` are produced from the Rust
DTOs by `ts-rs` during `cargo test` and are never edited by hand.

The embedded `index.html` is served for `/`, `/admin`, `/admin/*` and
`/portal`; hashed files under `/assets/` are cached for a year, the HTML is
`no-cache`, and `/build-info.js` injects the build identity.
