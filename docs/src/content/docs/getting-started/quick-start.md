---
title: Quick Start
description: "From a downloaded binary to a working gateway: start gproxy, create the administrator, add a provider and a route, issue a key, and send a request."
---

This page takes a fresh native installation to its first successful request.
It assumes a portable archive from the [Downloads](/getting-started/downloads/)
page. An installer performs steps 1 and 2 for you and opens the console.

## 1. Start gproxy

```bash
chmod +x ./gproxy
./gproxy
```

The server listens on `127.0.0.1:8787`, creates `./data/gproxy.db`, and logs
`GPROXY listening`. `gproxy --help` lists every flag. Each flag has a
`GPROXY_*` environment twin, and both can be written into a `.env` file in the
working directory or in the data directory. Precedence is flag, then
environment, then `./.env`, then `<data-dir>/.env`, then the default.

A minimal `.env`:

```env
GPROXY_HOST=127.0.0.1
GPROXY_PORT=8787
GPROXY_DATA_DIR=./data
GPROXY_MASTER_KEY=<standard base64, 32 bytes>
```

Generate the key with `openssl rand -base64 32`. Without it, credentials and
user keys are stored in plaintext. Set it before adding the first credential;
changing it afterwards is a rotation step described in
[Configuration](/reference/configuration/). Installers write a `.env` with a
generated key for you.

For a container:

```bash
docker run -d --name gproxy -p 8787:8787 \
  -v gproxy-data:/var/lib/gproxy \
  ghcr.io/leenhawk/gproxy:<tag>
```

## 2. Create the Administrator

Open <http://127.0.0.1:8787/admin>. On a fresh store the console shows
**Create the administrator**. Choose a username and a password; you are signed
in when the form completes. The console navigation is Overview, Providers,
Load balancing, Rules, Identity, Statistics, Pricing, Tokenizers, Updates, and
Settings.

The administrator can also be created from `GPROXY_ADMIN_PASSWORD`; see
[Installation](/getting-started/installation/#first-boot).

## 3. Add a Provider and a Credential

Go to **Providers → Add provider**. Enter a route name (the stable identifier
of this provider, also usable as a named-mode path prefix), pick the channel,
and choose the credential strategy: **Round robin** rotates requests across
the pool, **Sticky by API key** keeps each client key on one credential. The
channel decides which settings appear, for example a base URL for `custom` or
a region for `aws-bedrock`. Saving the provider seeds the channel's routing
rules and creates an empty private rule set named `<provider> · defaults`.

Then open the provider and choose **Add credential**. There are two ways to
supply the secret:

- **Paste it.** Pick the credential kind (API key, OAuth, or Cookie) and fill
  the fields the channel declares, or use the JSON field for the raw
  credential object. The label is optional; a default is derived from the
  secret.
- **Sign in.** Channels that declare a sign-in method show a **Sign-in
  method** selector. `codex` offers **Browser sign-in** (authorization code
  with PKCE) and **Device code**; `claudecode` offers **Browser sign-in** and
  **Browser cookie**. Start the sign-in, approve it in the browser, paste the
  callback URL (or enter the device code on the verification page), and
  complete it. The tokens are stored sealed and refreshed by GPROXY under an
  exclusive lease.

Each credential row carries a traffic weight, optional requests-per-minute and
tokens-per-minute limits, a proxy override, and its observed health.

Optionally open the provider's **Models** tab and use **Pull from upstream** to
record the model ids it serves, together with capabilities and default prices.

## 4. Create a Route

Go to **Load balancing → New load balancer**. Enter a route name and the
maximum number of attempts (the first attempt plus failovers). Then **Add
member**: choose the provider, type the upstream model id, optionally pin a
credential, and set the failover tier and weight. Tier 0 is exhausted before
tier 1 receives traffic; weight splits traffic among healthy members in the
same tier. Add members from other providers for failover.

Creating a load balancer does not expose it yet. Under **Model mappings**,
add a public model name that points at it; that name is what clients send as
`model`. Aggregated resolution runs alias, then variant suffix, then public
model name, then the load balancer's members. A route name on its own is
reachable only through the named prefix, `/{route}/v1/...`.

## 5. Create a User and an API Key

Go to **Identity** and create a user. A password is optional; it is needed
only if the user should sign in to the portal. Then, under the user's **API
keys**, choose **Create API key**: give it a label, pick the prefix — **Standard
key (sk-)** for API clients, **Codex key (at-)** for Codex CLI access-token
login — and an optional expiry. Copy the key when it is shown. The list shows
only the prefix afterwards; revealing the full key is a separate, audited
action.

Permissions are default-deny. Under **Access**, add a permission with effect
**Allow**, for all providers or one provider, and for all operations or one
operation group. It can be attached to the key, the user, a team, or an
organization and is inherited downward. Without an allow permission every
request from the key is refused with `403`. Rate limits and cost quotas are
added in the same place.

## 6. Send a Request

Replace the placeholders with the key and the public model name:

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "<public-model-name>",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

The response carries an `x-request-id` header. Open **Statistics → Request
audit** in the console to see the request and the upstream call it produced.

## Next Steps

- [First Request](/getting-started/first-request/) shows the same call in
  every accepted format, streaming, model listing, and the named prefix.
- Users with a password can sign in at `/portal` to create their own keys and
  copy connection snippets for curl, the OpenAI, Claude, and Gemini SDKs,
  Codex CLI, and Claude Code. See
  [Console, Portal & Public Site](/guides/console/).
- [CLI Clients](/guides/cli-clients/) covers pointing Codex CLI and Claude
  Code at the gateway.
