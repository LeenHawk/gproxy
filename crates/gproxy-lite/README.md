# gproxy-lite

A thin GPROXY engine for serverless and edge platforms. It reuses the stateless
crates (`gproxy-protocol`, `gproxy-client`, `gproxy-upstream`, `gproxy-channel`,
`gproxy-convert`) and nothing above them: no database, one operator, one access
key.

| Module | Role |
|---|---|
| `config` | The configuration JSON: the access key, and the providers with their channel, base URL, channel config and credential |
| `auth` | The access key a request presents, compared in constant time |
| `store` | The host's key-value interface (`KeyValue`, the contract of `gproxy_upstream::UpstreamState`), expiry on top of it, and the views channels and protocol conversion read through |
| `credential` | The credential of a provider: pasted or signed in, replaced with compare-and-exchange |
| `renew` | Renewal under a lease: before a request, after a `401`, and on the host's schedule (`Engine::renew_due`, `POST /lite/renew`) |
| `login` | `/lite/login/start`, `/lite/login/complete`, `/lite/status`, `/lite/renew` |
| `engine` | Addressing (`/{provider}/…`, `provider/model`) and the ingress table of `gproxy-convert` |
| `exec` | Passthrough or conversion, as `gproxy_convert::default_route_for_model` decides |
| `resources` | What a conversion may read: a provider file, a URL. Nothing is published |
| `websocket` | WebSocket operations, relayed |
| `wasm` | The WebAssembly module's exports (`wasm32` only) |

The JavaScript shell, the per-platform stores and the deployment templates are
in `deploy/lite`.
