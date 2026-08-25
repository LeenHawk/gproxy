# Deno bundle

Requirements: Deno, Rust wasm target, `wasm-pack`, Node.js, and pnpm.

```sh
deno task build
deno task check
deno task start
```

`GPROXY_CONFIG` contains the wasm TOML configuration and must be supplied by
the deployment secret manager. The build creates ignored `pkg/` and `public/`
directories; no generated wasm or frontend assets are committed.

`main.ts` is the static layer for `/`, the exact admin and portal roots,
`/assets/**`, and `/favicon.svg`. All other paths enter Rust. Deno's native
`upgradeWebSocket` is consumed by the Rust inline helper. A module-level set
retains each returned continuation without awaiting it, so the HTTP upgrade
response is returned immediately while WebSocket pumping remains live.
