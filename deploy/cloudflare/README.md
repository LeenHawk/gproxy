# Cloudflare Workers deployment

The v4 edge host is `crates/gproxy-host-edge`: a Workers `fetch` handler over
the same axum router the native binary serves. See
[Edge deployment](https://gproxy.leenhawk.com/deployment/edge/) for the
configuration document, the secrets and what the Worker refuses.

The release bundle `gproxy-edge-cloudflare.zip` includes the built Worker and console.
Enter its `cloudflare/` directory and install the package dependencies before using Wrangler.
The Worker synchronizes the schema during first assembly. An empty database still
needs compatible identity data: this host has no first-administrator setup flow.
See the deployment guide above before deploying a fresh instance.

```sh
pnpm install
```

With Wrangler available:

```sh
pnpm exec wrangler d1 create gproxy               # paste the id into wrangler.toml
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler deploy
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
