# Cloudflare Workers bundle

Requirements: Rust wasm target, `wasm-pack`, Node.js, and pnpm.

```sh
pnpm install
pnpm run build
pnpm run check
pnpm exec wrangler secret put GPROXY_LIBSQL_URL
pnpm exec wrangler secret put GPROXY_LIBSQL_AUTH_TOKEN
pnpm run dev
```

The Worker constructs the typed edge config from those bindings. Optional
secret-at-rest bindings are `GPROXY_MASTER_KEY`, `GPROXY_MASTER_KEY_NEXT`, and
`GPROXY_MASTER_KEY_ROTATE`; store key values only as Workers secrets. The build
creates ignored `pkg/` and `public/` directories; no generated wasm or frontend
assets are committed.

The Worker sends only `/`, the exact admin and portal roots, `/assets/**`, and
`/favicon.svg` to the `ASSETS` binding. Admin APIs, portal APIs, provider
surfaces, and inference paths go through Rust. WebSocket continuations are
registered with `ExecutionContext.waitUntil` before the response is returned.
