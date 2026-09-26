# Cloudflare Workers deployment

The v4 edge host is `crates/gproxy-host-edge`: a Workers `fetch` handler over
the same axum router the native binary serves. See
[Edge deployment](https://gproxy.leenhawk.com/deployment/edge/) for the
configuration document, the secrets and what the Worker refuses.

From a release's `gproxy-edge-cloudflare.zip`, everything is built:

```sh
wrangler d1 create gproxy               # paste the id into wrangler.toml
wrangler d1 migrations apply gproxy --remote
wrangler secret put GPROXY_MASTER_KEY
wrangler deploy
```

From a checkout, build first. It needs the `wasm32-unknown-unknown` target,
`worker-build` (`cargo install worker-build`), Node.js and pnpm:

```sh
pnpm install
pnpm run build      # the Worker into build/, the console into public/
pnpm run deploy
```

The console is served by Workers Assets from `public/console/` and never wakes
the Worker; every other path, provider mounts included, goes to the Worker
first. `_headers` gives the console the security headers the native host sends.
