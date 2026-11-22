# gproxy-lite on Cloudflare Workers, Netlify and Vercel

`gproxy-lite` is the engine these platforms run: single-tenant, no database.
What it is and what it leaves out is described in the
[documentation](https://gproxy.leenhawk.com/deployment/lite/).

| Path | What it is |
|---|---|
| `build.sh` | Builds the WebAssembly module and copies it, with the shell, into each template's `lite/` |
| `shell/gateway.mjs` | Starts the engine, hands it requests and runs the scheduled renewal |
| `shell/stores/` | The key-value store of each platform |
| `cloudflare/` | Worker, with a Durable Object as the store |
| `netlify/` | Function, with Netlify Blobs as the store |
| `vercel/` | Function, with Upstash Redis as the store |

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130   # the version in Cargo.lock
sh deploy/lite/build.sh
```

`wasm-opt` (Binaryen) is used when it is on `PATH`. Every channel is built in.

## Configuration

One environment variable, `GPROXY_LITE_CONFIG`, holds the JSON:

```json
{
  "access_key": "what clients send as their API key",
  "providers": [
    { "name": "openai", "channel": "openai", "credential": "sk-..." },
    { "name": "codex", "channel": "codex" }
  ]
}
```

It contains secrets; store it as one on every platform.

## Staying signed in

A sign-in is renewed by the requests that use it and, so that it outlives a
period without requests, on a schedule: once a day at 04:00 UTC in every
template. `POST /lite/renew` with the access key does the same on demand and
answers with the outcome per provider. A run in which a renewal failed is
logged as failed.

| Platform | What runs it | Where the schedule is | What it needs |
|---|---|---|---|
| Cloudflare | Cron Trigger, the Worker's `scheduled` handler | `[triggers] crons` in `cloudflare/wrangler.toml` | Nothing. The Free plan has five Cron Triggers per account |
| Netlify | Scheduled Function `functions/renew.mjs` | `[functions."renew"] schedule` in `netlify/netlify.toml` | A published deploy; a run lasts at most 30 seconds |
| Vercel | Cron Job requesting `GET /lite/renew` | `crons` in `vercel/vercel.json` | `CRON_SECRET`; Hobby runs a job at most once a day, within the hour |

Change the cron expression and deploy again. Once a day keeps a refresh token
alive; a shorter interval only matters for a session that ends sooner than
that, and Vercel Hobby refuses one.

## Cloudflare Workers

```sh
cd deploy/lite/cloudflare
wrangler secret put GPROXY_LITE_CONFIG
wrangler deploy
```

For a local run, copy `dev.vars.example` to `.dev.vars` and use `wrangler dev`;
`curl "http://localhost:8787/cdn-cgi/local/scheduled"` runs the scheduled
renewal.

## Netlify

```sh
cd deploy/lite/netlify
npm install
netlify env:set GPROXY_LITE_CONFIG '...' --secret
netlify deploy --prod
```

A Netlify function runs for at most 60 seconds, streaming included, and cannot
serve WebSockets.

## Vercel

Add Upstash Redis to the project from the Marketplace; it sets
`KV_REST_API_URL` and `KV_REST_API_TOKEN`. Then:

```sh
cd deploy/lite/vercel
vercel env add GPROXY_LITE_CONFIG
vercel env add CRON_SECRET          # a random string of 16 characters or more
vercel deploy --prod
```

Vercel sends `CRON_SECRET` as the bearer token of the scheduled request, and
the function accepts that request only with it. Without the variable the
scheduled request is refused and sign-ins are renewed by requests alone.

The shell does not serve WebSockets on Vercel.
