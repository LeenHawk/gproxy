# Cloudflare Workers deployment

The v4 edge host is `crates/gproxy-host-edge`: a Workers `fetch` handler over
the same axum router the native binary serves. See
[Edge deployment](https://gproxy.leenhawk.com/deployment/edge/) for the
configuration document, the secrets and what the Worker refuses.

The release bundle `gproxy-edge-cloudflare.zip` includes the built Worker and console.
Enter its `cloudflare/` directory and install the package dependencies before using Wrangler.
The Worker synchronizes the schema during first assembly. Set `GPROXY_ADMIN_PASSWORD` before the first request. It creates the initial
`admin` user only when the database has no users; `GPROXY_ADMIN_USER` can change
the name. On subsequent starts, a user matching `GPROXY_ADMIN_USER` takes priority: only their password is updated. If no name matches, user `0` is enabled as an administrator and given the configured name and password; user `0` is created if missing.
Unchanged credentials preserve sessions; a password change or recovery ends them.
Without the secret, an existing instance keeps its stored password. Sign in at `/console/`
and create a gateway API key after deployment.

```sh
pnpm install
```

With Wrangler available:

```sh
pnpm exec wrangler d1 create gproxy               # paste the id into wrangler.toml
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler secret put GPROXY_ADMIN_PASSWORD
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
