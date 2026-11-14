# AGENTS.md: building a custom GPROXY console

This file is for a coding agent building a replacement web console for a
GPROXY instance. The administrator uploads the finished build as a zip in the
built-in console under Settings > General > Custom console, and GPROXY then
serves it at `/console/` in place of the built-in console. Read the whole file
before writing code. The rules marked MUST are not optional: section 3 keeps
the administrator able to recover the instance, and section 4 keeps the
author's copyright and license notice intact.

## 1. What you are building

A static single-page application. GPROXY has no server-side rendering and runs
no code from the bundle; it only serves files. The deliverable is one zip:

```
index.html          <- required, at the zip root (or inside one top-level folder such as dist/)
assets/...          <- everything index.html loads
```

Limits enforced on upload: zip body at most 50 MiB, at most 20,000 files, at
most 256 MiB extracted. Symbolic links, absolute paths, `..`, `.` and empty
path segments, backslashes and `:` in a path are rejected. `__MACOSX/` and
`.DS_Store` entries are skipped.

## 2. Hosting rules (MUST)

1. **Base path is `/console/`.** Every asset URL must resolve under
   `/console/`. With Vite set `base: "/console/"`; with other tools set the
   equivalent public path. Relative `./assets/...` URLs break on nested routes.
2. **Client-side routing uses history paths under `/console`.** Any GET for an
   extensionless path below `/console/` (for example `/console/keys/42`)
   returns `index.html`. A path whose last segment has a dot (`app.js`,
   `logo.svg`) is a file request and returns 404 when missing, never
   `index.html`.
3. **Content Security Policy.** Every console response carries:

   ```
   default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
   img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self';
   object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'
   ```

   Consequences:
   - No inline `<script>` and no `eval`/`new Function`. Frameworks that inline a
     bootstrap or hydration script in `index.html` (Next.js static export,
     SvelteKit, some Astro and Nuxt setups) will render a blank page. Plain
     Vite + React/Vue/Svelte/Solid with an external module script works.
   - No CDN scripts, fonts, stylesheets or analytics. Bundle everything.
   - `fetch` only to the same origin. Do not call third-party APIs.
   - Inline `style` attributes are fine.
   - The console cannot be framed.
4. **Caching.** Uploaded files are served with `Cache-Control: no-cache`, so
   unhashed file names work, but hashed names are still recommended.
5. **Reserved paths.** `/console/fonts/active.css` and `/console/fonts/*.woff2`
   are answered by GPROXY's optional font cache, not by your bundle. Do not put
   files there.

## 3. The settings control you MUST keep

The custom console replaces the only UI that can remove it. It MUST therefore
include, on its system settings page (the page an instance administrator
opens for instance settings), a "Custom console" section that offers:

- **Status**: whether a custom bundle is installed, its upload time, file
  count and size (`GET /admin/api/console-bundle`).
- **Upload / replace**: a file input accepting `.zip` that sends the file as
  the raw request body (`POST /admin/api/console-bundle`), then reloads
  `/console/` on success.
- **Restore the built-in console**: a button that sends
  `DELETE /admin/api/console-bundle`, confirms first, then navigates to
  `/console/`.
- **Open the built-in console**: a plain link to `/console/?console=builtin`.

Show this section only to callers whose admin context includes the `settings`
section (instance administrators). Do not hide it behind a feature flag, a
build option or a second page that might be removed later.

Recovery that works even if your build is broken: opening
`/console/?console=builtin` sets a cookie that makes this browser receive the
built-in console; `/console/?console=custom` clears it. Mention this URL in
the settings section so the administrator knows it exists. It only works when
the GPROXY binary has a built-in console (`builtin: true` in the status).

## 4. The About page you MUST keep

GPROXY is maintained by Leen Hawk and licensed under AGPL-3.0-or-later. A
custom console MUST keep an About page, reachable from the navigation, that
shows the following content. You may change the layout, styling, order of
blocks, icons and components freely, but the wording, names, version line and
links must stay as given. Do not shorten, paraphrase, rebrand or hide any of
it (no collapsed sections, no `display: none`, no text the same colour as the
background).

Required text (English):

- Title: `About GPROXY`
- `One endpoint for your LLM providers, accounts and applications.`
- Product name `GPROXY`, with the version and build hash from `GET /info`
  (`version`, `hash`), for example `Version 4.2.3 · eb7e643a1`.
- `GPROXY is an open-source, self-hosted LLM API gateway built with Rust. Connect your applications to multiple providers through one API endpoint.`
- Feature list:
  - `Use OpenAI Chat Completions, Responses, Claude Messages and Gemini APIs, with streaming and direct format conversion.`
  - `Pool upstream credentials with token refresh, load balancing and failover.`
  - `Manage access, quotas, usage and costs through the admin console and user portal.`
- `Maintained by Leen Hawk and the community. The gateway is licensed under AGPL-3.0-or-later; see the repository for individual component licenses.`
- Links, opening in a new tab (`target="_blank" rel="noopener noreferrer"`):
  - `Source code`: https://github.com/LeenHawk/gproxy
  - `Documentation`: https://gproxy.leenhawk.com
  - `Report an issue`: https://github.com/LeenHawk/gproxy/issues
- Sponsor block:
  - Title: `Support GPROXY`
  - `Help GPROXY reach mobile app stores: Google Play costs $25 once, and the Apple Developer Program costs $99 per year. Once approved and listed, GPROXY can be installed and updated through the stores.`
  - `Your support helps sustain maintenance, provider compatibility and new features. Thank you for using and supporting GPROXY.`
  - Link `Sponsor LeenHawk`: https://github.com/sponsors/LeenHawk

If your console is translated, use the official translations of these strings
under the `about` key in the built-in console's locale files
(https://github.com/LeenHawk/gproxy/tree/main/console/src/locales, file
`pages.json`, languages `en`, `zh-CN`, `zh-TW`, `ru`) instead of translating
them yourself. For a language without an official translation, show the
English text.

You may add your own credits (for example "Custom console by ...") as long as
they are visually separate from the text above and do not replace it.

## 5. Talking to GPROXY

All APIs are same-origin JSON over HTTPS. Send `credentials: "same-origin"`
(the default for same-origin `fetch`). Never store a bearer token in
`localStorage` or `sessionStorage`; the session is an `HttpOnly` cookie.

### Errors

Every non-2xx answer with a body uses one envelope:

```json
{ "error": { "code": "forbidden", "message": "..." } }
```

Branch on `code` and the HTTP status; show `message` as is (it is already safe
to display). A body may also be absent or not JSON (a reverse proxy's 502), so
parse defensively. `204` has no body.

- `401`: no session or it expired. Go to the sign-in screen.
- `403`: signed in but not allowed. Do NOT sign the user out.
- `400` with an admin route: often a missing admin scope, see below.

### Session

- Sign in: `POST /portal/api/login` with
  `{"name": "...", "password": "..."}` (`content-type: application/json`).
  The response sets the session cookie. Ignore the `token` field in the body.
- Who am I: `GET /portal/api/context` (every signed-in user) and
  `GET /admin/api/context` (administrators; `403` means the user administers
  nothing, which is a normal answer, not an error to show).
- Sign out: `POST /portal/api/logout`.

Unsafe methods (`POST`, `PUT`, `PATCH`, `DELETE`) authenticated by the cookie
are checked for same origin. Browsers send `Origin` automatically; do not try
to set it.

### Admin scope

`GET /admin/api/context` returns `scopeHeader` (currently
`x-gproxy-admin-scope`), `scope`, `scopes` and `sections`. An instance
administrator has one scope and needs nothing more. An organization or team
administrator may have several; when `scope` is `null`, every other admin
route answers `400` until you send the chosen scope in the header named by
`scopeHeader` on each `/admin/api/*` request. Build the navigation from
`sections` (each has `id`, a collection `path` under `/admin/api`, and
`capabilities` of `read` and/or `write`). Do not show pages for sections the
caller lacks.

### Surfaces

- `/admin/api/*`: management (providers, credentials, models, routes, users,
  keys, organizations, quotas, usage, settings, transfer, audit ...).
- `/portal/api/*`: what an ordinary signed-in user sees about themselves
  (their keys, usage, subscriptions).
- `GET /info`: public instance name and version, no auth.
- `GET /healthz`: liveness.

### Custom console bundle API

Instance administrators only (`settings` section).

| Method | Path | Body | Answer |
| --- | --- | --- | --- |
| `GET` | `/admin/api/console-bundle` | none | status |
| `POST` | `/admin/api/console-bundle` | the zip file, raw, `content-type: application/zip` | status |
| `DELETE` | `/admin/api/console-bundle` | none | status |

Status:

```ts
type ConsoleBundleStatus = {
  installed: boolean        // a custom bundle is serving /console
  builtin: boolean          // the binary has a built-in console (recovery link works)
  installedAt: number | null // unix seconds
  files: number
  bytes: number
}
```

A rejected upload answers `400` with the reason in `error.message`, and the
previous bundle keeps serving.

### Full API reference

- Human reference: https://gproxy.leenhawk.com/reference/http-api
- OpenAPI document: https://gproxy.leenhawk.com/generated/openapi.json
- TypeScript types generated from the server's DTOs, used by the built-in
  console: https://github.com/LeenHawk/gproxy/tree/main/console/src/generated
- The built-in console's source is the best example of every call:
  https://github.com/LeenHawk/gproxy/tree/main/console/src/api

Prefer copying the generated types over writing your own. Do not invent
fields; if a field you need is not in the OpenAPI document, it does not exist.
Send only the fields the user changed in a `PATCH`.

## 6. Secrets and safety

- API keys and credential secrets are shown once at creation. Do not log them,
  put them in URLs, or keep them in persistent storage.
- Confirm destructive actions (delete, revoke, restore built-in console).
- The console runs with the signed-in user's full authority. Do not add
  telemetry, remote error reporting or any network call off the origin.

## 7. Before you hand over the zip

Check each item:

- [ ] `index.html` is at the zip root (or in one top-level folder).
- [ ] Opening `/console/` and a deep link such as `/console/settings` both work
      after a hard reload.
- [ ] The browser console shows no CSP violations.
- [ ] No request leaves the origin (check the network panel).
- [ ] Sign in, sign out and an expired session (`401`) all behave.
- [ ] The system settings page has the Custom console section from section 3,
      and its Restore button works.
- [ ] `/console/?console=builtin` is shown to the administrator.
- [ ] The About page from section 4 is reachable from the navigation and its
      text and links match word for word.

To test locally, run a GPROXY instance, upload the zip from the built-in
console, and use `/console/?console=builtin` to get back.
