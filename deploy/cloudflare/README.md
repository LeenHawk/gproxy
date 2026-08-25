# Cloudflare Workers bundle

Requirements: Rust wasm target, `wasm-pack`, Node.js, and pnpm.

```sh
pnpm install
pnpm run build
pnpm run check
pnpm exec wrangler secret put GPROXY_CONFIG
pnpm run dev
```

`GPROXY_CONFIG` is the complete wasm TOML configuration. Store it only as a
Workers secret. The build creates ignored `pkg/` and `public/` directories;
no generated wasm or frontend assets are committed.

The Worker sends only `/`, the exact admin and portal roots, `/assets/**`, and
`/favicon.svg` to the `ASSETS` binding. Admin APIs, portal APIs, provider
surfaces, and inference paths go through Rust. WebSocket continuations are
registered with `ExecutionContext.waitUntil` before the response is returned.
