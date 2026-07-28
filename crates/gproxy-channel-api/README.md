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
```

See the GPROXY `adding-a-channel` guide for a complete adapter and custom binary.
