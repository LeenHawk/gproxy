# gproxy-channel-api

Contracts for implementing compile-time [GPROXY](https://gproxy.leenhawk.com)
channel extensions.

The crate exposes channel routing, request preparation, shaping, interactive
login, credential refresh, usage parsing, streaming, and host transport types.
It does not expose persistence, encryption, proxy selection, or the GPROXY HTTP
server.

```toml
[dependencies]
gproxy-channel-api = { version = "2", features = ["external-channels"] }
linkme = "0.3"
```

The registration slice is native-only. Use the API release and package source
that match the GPROXY source or tag linked by the custom runner; two copies from
different Cargo sources have separate registration slices.

See the [Adding a Channel](https://gproxy.leenhawk.com/guides/adding-a-channel/)
guide and the checked `examples/external-channel/` workspace for a complete
adapter, link-retention test, and custom binary.
