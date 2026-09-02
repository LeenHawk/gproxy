# Deno bundle

Requirements: Deno, Rust wasm target, `wasm-pack`, Node.js, and pnpm.

```sh
deno task build
deno task check
deno task start
```

`GPROXY_LIBSQL_URL` and `GPROXY_LIBSQL_AUTH_TOKEN` are required deployment
bindings. `GPROXY_MASTER_KEY`, `GPROXY_MASTER_KEY_NEXT`, and
`GPROXY_MASTER_KEY_ROTATE` are optional secret-at-rest and rotation bindings.
`UPSTASH_URL` and `UPSTASH_TOKEN` optionally select the shared Upstash cache;
set both or neither. The host builds a typed edge config from these bindings.
The build creates ignored `pkg/` and `public/` directories; no generated wasm
or frontend assets are committed.

`main.ts` is the static layer for `/`, the exact admin and portal roots,
`/assets/**`, and `/favicon.svg`. All other paths enter Rust. Deno's native
`upgradeWebSocket` is consumed by the Rust inline helper. A module-level set
retains each returned continuation without awaiting it, so the HTTP upgrade
response is returned immediately while WebSocket pumping remains live.
