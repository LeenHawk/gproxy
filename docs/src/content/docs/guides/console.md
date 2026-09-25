---
title: Console and Scoped Administration
description: "Manage credentials, upstream sign-in, routes, quotas, pricing and configuration transfer in the v4 Console."
---

The v4 Console is served at `/console`. One application contains personal
pages and administrative pages. Sign in using the deployment's configured
account; the Console uses `/portal/api/login` and an HttpOnly session cookie.

Personal cost budgets are displayed read-only on Overview, alongside usage.
There is no separate personal quota page; administrators edit budgets on the
corresponding user, API key, organization or team.

## Management Scope

The Console combines personal features from `/portal/api/context` with the
management sections returned by `/admin/api/context`. Instance administrators
manage the gateway. Organization and team administrators use the same Providers
entry for their credentials. Their Organizations and Teams pages show only
admitted objects and cost budgets, without identity CRUD or provider configuration.

If you administer multiple organizations or teams, choose a scope in the
account menu. Management requests send the server-provided scope selector in
`x-gproxy-admin-scope`. Switching scope closes open editors and login flows
and replaces the scope's query cache. A pending write must finish before
switching. Personal pages remain available independently of management scope.

## Configuration Pages

| Console path | Capability |
| --- | --- |
| `/console/providers` | Provider configuration, credentials, models, protocol conversion, rewrite bindings and endpoint overrides |
| `/console/model-routes` | Cross-provider routes, members, weights, fallback tiers and public model names |
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

Open the provider's Credentials tab to manage its credentials. New credentials
and sign-in flows inherit that provider. Ownership choices
are restricted to the current administrative scope. Tenant directories expose
provider labels and supported actions, not gateway configuration.

Editing a credential opens one panel with **Basic settings**, **Local limits**
and **Upstream allowance** tabs. The credential row offers Upstream allowance, a
compact model test dialog, and deletion. Secret
reveal and refresh sit below the secret field in Basic settings; lifecycle
status and health reset sit in the status area. The test dialog loads available
models and shows the result, latency, and reply. Upstream readings are presented as amounts and reset
times; generation tests make a real upstream request.

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

Administrators can set a key budget directly below its name in the **New key**
form, both in personal Keys and in User keys. The key and initial budget commit
in one transaction; invalid budgets create no key. Existing keys open a single
editor with **Basic settings** and **Cost budget** tabs. Ordinary users keep
the self-service key flow; administrators manage their budgets.

Open **Cost budget** on a user, API key, organization or team. Rules are added
and edited inside that panel, without another dialog. Costs remain decimal
strings in USD; provider/credential request limits use unit `count`.

A provider's **Settings → Default credential limits** applies each rule to
**each credential separately**, not to a shared provider budget. The page shows
configuration; individual credentials show usage and reset times. An enabled
credential rule with the same rule name overrides the provider default.
**Restore inheritance** deletes that credential override. Disabling an override
also allows an enabled default to apply again; it does not mean unlimited use.

Credential limit cards show their source, configured cap, actual usage and next
reset. Tenant administrators can read effective credential limits; instance
administrators configure operator limits. Period, amount/count, model pattern
and enablement are edited in place. Custom duration and fixed-period anchors
are advanced options.

Resetting a provider default affects every credential currently inheriting it,
so its confirmation names that scope. An inherited card has no credential-only
reset button. Credential overrides and caller budgets reset only their own
rule. None of these actions resets the upstream account allowance.

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
