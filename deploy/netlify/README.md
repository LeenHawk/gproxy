# Netlify Edge bundle

Requirements: Rust wasm target, `wasm-pack`, Node.js, and pnpm.

```sh
pnpm install
pnpm run build
pnpm run check
pnpm run dev
```

Configure `GPROXY_CONFIG` as a sensitive environment variable in Netlify. It
contains the complete wasm TOML configuration and must not be committed. The
build publishes the generated `public/` directory and packages the ignored
`pkg/` wasm output with the Edge Function.

The Edge Function is configured for all non-static paths; the public root,
exact admin and portal roots, `/assets/**`, and `/favicon.svg` stay on Netlify's
static layer. The entry uses `Context.ip` as the trusted client source and
registers any continuation with `Context.waitUntil`.

No WebSocket capability or shim is installed by this bundle. On Netlify's
runtime, WebSocket-intent requests therefore reach Rust and receive its explicit
501 response instead of silently degrading to HTTP.
