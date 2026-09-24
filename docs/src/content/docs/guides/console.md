---
title: Console and Scoped Administration
description: "Manage credentials, upstream sign-in, routes, quotas, pricing and configuration transfer in the v4 Console."
---

The v4 Console is served at `/console`. One application contains personal
pages and administrative pages. Sign in using the deployment's configured
account; the Console uses `/portal/api/login` and an HttpOnly session cookie.

## Management Scope

The Console combines personal features from `/portal/api/context` with the
management sections returned by `/admin/api/context`. Instance administrators
manage the gateway. Organization and team administrators manage credentials
and quotas within their current scope.

If you administer multiple organizations or teams, choose a scope in the
account menu. Management requests send the server-provided scope selector in
`x-gproxy-admin-scope`. Switching scope closes open editors and login flows
and replaces the scope's query cache. A pending write must finish before
switching. Personal pages remain available independently of management scope.

## Configuration Pages

| Console path | Capability |
| --- | --- |
| `/console/providers` | Provider configuration, credentials, models, protocol conversion, rewrite bindings and endpoint overrides |
| `/console/credentials` | Credentials grouped by provider, including scoped tenant administration |
| `/console/model-routes` | Cross-provider routes, members, weights, fallback tiers and public model names |
| `/console/quotas` | Caller budgets and provider/credential limits |
| `/console/rule-sets` | Rewrite rules, ordering and whole-set saves |
| `/console/transfer` | Configuration export and import |
| `/console/clients` | Reusable connection profiles |
| `/console/settings` | Instance and logging settings |
| `/console/tokenizer` | Vocabulary downloads and model vocabulary bindings |
| `/console/update` | Available update operations for the host |

The provider's **Routing** tab controls operation/protocol conversion. **Model
routes** choose among upstream providers; these are separate settings.

Configuration tables support current-page selection and batch deletion; rows
with an enabled flag also support batch enable/disable. Filtering, changing
page, or changing management scope clears the selection. Each batch uses the
existing transactional configuration API.

## Credentials and Upstream Sign-in

Open a provider's credentials, or choose its group on the Credentials page.
New credentials and sign-in flows inherit that provider. Ownership choices
are restricted to the current administrative scope. Tenant directories expose
provider labels and supported actions, not gateway configuration.

The details action offers saved quota observations, local limits, lifecycle
status, secret reveal, and the refresh/query/reset operations supported by the
channel. Upstream quota reset, local limit reset and health reset are separate
actions. Model discovery and generation tests can target a particular
credential. Generation tests make a real upstream request.

**Add by signing in** offers the channel's supported methods:

- Browser authorization: start, open the authorization link, then paste the
  full callback URL including `code` and `state`. A registered loopback
  callback may fail to load in the browser; copy its address anyway. This
  Console does not run a local callback listener.
- Device code: open the verification link and enter the displayed code. The
  Console polls at the interval returned by the backend and stops on success,
  denial, expiry or closing the window.
- Cookie exchange: submit an existing browser session cookie to the channel.

Login sessions are bound to the initiating user, administrative scope and
credential owner. Completion returns a credential ID, not its secret. Closing
an unfinished flow stops the UI; cached sessions expire naturally. Refreshing
the page does not restore the login wizard.

All upstream login routes are POSTs under `/admin/api/credential-login`:
`/authcode/start`, `/authcode/complete`, `/device/start`, `/device/poll` and
`/cookie/exchange`. They use the credentials section's scope checks. Scoped
credential discovery and testing use
`/admin/api/credentials/{id}/models/discover` and `/models/test` respectively.

## Quotas and Prices

Caller budgets support user, API key, organization, team and pool ownership.
Provider and credential limits also support request counts. The editor sends
costs as decimal strings in USD and request counts with unit `count`. Period,
anchor, custom duration and model pattern use the existing backend contract.
Pool IDs are entered explicitly; there is no separate pool directory.

User, API key, organization, team, provider and credential views include a
quota shortcut. Budget reset calls `/quotas/{id}/reset`; operator limit reset
calls `/quotas/{id}/limit-reset` and does not reset an upstream account.

Price rules may apply to all providers or one provider, and model patterns
support `*` and `?`. Multiple operation-specific rules can coexist. Edit global prices from the Models page and provider-specific prices from a
provider's Models tab. These existing model dialogs manage rules, dimensional
rates and context/service tiers together.

## Import and Export

The transfer document contains gateway configuration, not identities, usage
or request logs. Export omits credential secrets by default. Including secrets
exports the stored sealing format. Import accepts the current v4 envelope.

The file summary shows version and record counts; it is not a server-side dry
run. **Merge** updates matching IDs and keeps unmentioned records. **Replace**
also deletes unmentioned records of the exported configuration kinds.

For encrypted secrets from another master key, supply the source key so the
backend can reseal them. Without it, encrypted blobs require the same key at
the destination. The UI clears the source key after submission.

Import is transactional. After it commits, the host reloads runtime settings.
Read the returned warnings and skipped counts: a reload failure is reported
as an already-imported configuration with a runtime warning, not as a rolled
back import.

## Building the Console

Run `pnpm --dir console lint`, `pnpm --dir console test`, and
`pnpm --dir console build`. The build type-checks, bundles with Vite and copies
assets to the embedded Console directory. Native binary builds can then serve
that bundle under `/console`; Edge deployments serve the same bundle as static
assets in front of the gateway.
