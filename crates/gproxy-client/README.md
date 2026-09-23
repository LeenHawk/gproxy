# gproxy-client

English | [简体中文](README.zh-CN.md)

Reusable native outbound clients for GPROXY v4. The `reqwest` feature is enabled
by default; enable `wreq` for TLS/HTTP emulation and `reqwest-native` for reqwest
0.12 over the platform's native TLS (OpenSSL on Linux, Secure Transport on macOS,
SChannel on Windows: the Codex CLI's HTTP stack, `Backend::ReqwestNative`). All
three can coexist. This crate does not depend on store or protocol, and does not
route requests or select credentials.

```rust,no_run
use gproxy_client::{Client, ClientPool, ConnectionConfig};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let pool = ClientPool::default(); // keep one pool for the host lifetime
let client = pool.get(&ConnectionConfig::default()).await?;
if let Client::Reqwest(http) = client.as_ref() {
    let response = http.get("https://example.com").send().await?;
    // Native response API: inspect status/headers, consume bytes or stream chunks.
    println!("{}", response.status());
}
# Ok(())
# }
```

`Client` also implements the [`OutboundClient`] transport contract that channels and
core call: `send` takes a prepared absolute `http::Request<HttpBody>` and returns the
response with a streaming body (non-2xx included); `connect` performs the WebSocket
upgrade and returns either the duplex socket or the rejected handshake response.
The upgrade layer owns the handshake headers, so caller-supplied `Upgrade`,
`Connection`, `Sec-WebSocket-Key/Version/Extensions` are dropped and
`Sec-WebSocket-Protocol` is turned into the protocol list. With the wreq backend a
rejected handshake keeps status and headers but not its body. The trait itself is
available on every target; only the implementation is native.

`Client` exposes the native backend handles, including streaming multipart APIs.
Reqwest WebSockets use the re-exported `reqwest_websocket` extension; wreq provides
its native WebSocket API. There is no second buffering or wire-protocol abstraction here.
## wasm32

The same `Client`, `ClientPool` and `OutboundClient` exist on wasm32, organised
by feature rather than by platform crate:

| Feature | Transport | Bodies | WebSocket |
|---|---|---|---|
| `fetch` | The JS host's global `fetch` through web-sys (Cloudflare Workers, Deno, Netlify Edge, browsers) | Streams both ways (`duplex: half`) | Needs `workers` or a host client |
| `workers` (implies `fetch`) | Same, plus Cloudflare Workers upgrades: `fetch` with `Upgrade: websocket`, then `response.webSocket` is accepted and exposed as the protocol socket | Streams | Yes, with upstream authentication headers |
| `reqwest` (default) | reqwest's own Fetch fallback | Request bodies are buffered | No |

When more than one is enabled `fetch` wins. Profiles still select clients, but
custom fingerprint headers are applied where the host accepts them. Native
proxy, TLS and socket settings are skipped. Fetch maps zero redirect hops to
manual redirects and positive values to host-managed following; the host sets
the actual hop limit. A host with its own transport (Workers
`Fetch` with service bindings and `cf` options, Deno `createHttpClient` with
proxies or CAs, or a `wasi:http` shim on other targets) implements
`OutboundClient` and installs it with `ClientPool::with_client` or a per-profile
`ClientPool::with_factory`; core then never touches the built-in transports.
`wasm32-wasip2` has no JS `fetch` and is not covered by these features.

Feature `libsql` (any target) makes `Client` the HTTP leg of gproxy-seaorm's
libSQL/Turso connection, so the database shares the outbound transport.

## Multipart and WebSocket

Both backends enable `multipart` and `stream`. Use their `multipart::Form` and
`multipart::Part::stream(Body::wrap_stream(...))` APIs through the cached handle;
this supports uploads without a known content length or buffering the whole file.

For RFC 6455 WS/WSS use `pool.get_websocket(&config).await?`. It retains the same
proxy, TLS/emulation and policy settings, but forces HTTP/1.1 (including TLS ALPN)
and caches that transport separately from regular HTTP. This prevents an HTTP/2
TLS negotiation from breaking the HTTP/1.1 Upgrade handshake.

- Reqwest: import `gproxy_client::reqwest_websocket::Upgrade`, then call
  `http.get(url).bearer_auth(token).upgrade().protocols(["protocol"]).send().await?`.
- Wreq: call `http.websocket(url).bearer_auth(token).protocols(["protocol"]).send().await?`.

Inspect the upgrade response, then call `into_websocket().await?`. Both return
bidirectional text/binary streams with close support. An upgraded socket belongs
to that WS session; caching Clients does not reuse an existing logical WS session.
Clearing the Client cache does not interrupt an already upgraded stream.

## Profiles and effective parameters

Store persists named `ConnectionProfile` rows. The host selects the first explicit
`connection_profile_id` from Credential → Provider, then the channel's own
`default_connection()` when it has one, then the Setting default; it loads that
complete profile and maps its fields into `ConnectionConfig`. An absent selection
uses `ConnectionConfig::default()` (reqwest, direct). A missing referenced row is
an error, never a reason to fall back.
No per-field merging or profile-to-profile inheritance is performed.

| Store fields | Client configuration |
|---|---|
| `backend` | `Backend::Reqwest` / `Backend::Wreq` / `Backend::ReqwestNative` (`reqwest_native`) |
| `proxy_mode = direct/system` | `ProxyConfig::Direct` / `System`; `proxy_url` must be null |
| `proxy_mode = explicit`, `proxy_url` | `ProxyConfig::Explicit { url }`; URL required |
| `emulation` JSON / null | Deserialize `EmulationConfig` (`kind: preset` or `kind: custom`; the flat preset object of earlier releases still loads) / `None` |
| `gzip`, `brotli`, `deflate`, `zstd` | Independent automatic decompression switches, default false |
| `redirect_max_hops` | 0 disables following; positive values limit redirect hops |
| `retry` | `never` (default) or `default` (native backend retry policy) |
| `connect_timeout_ms`, `pool_idle_timeout_ms`, `pool_max_idle_per_host` | Same fields |

This mapping/inheritance belongs to the future host management/execution layer;
the client crate does not query the database. Store remains an entity draft.
URLs containing proxy authentication are secret configuration. Do not expose
serialized profiles or entity Debug output to ordinary users or logs.

Example effective configuration:

```json
{
  "backend": "wreq",
  "proxy": { "mode": "explicit", "url": "socks5h://127.0.0.1:1080" },
  "emulation": {
    "kind": "preset",
    "profile": "chrome_133",
    "platform": "linux",
    "http2": true,
    "headers": false
  },
  "gzip": true,
  "brotli": true,
  "deflate": true,
  "zstd": true,
  "redirect_max_hops": 5,
  "retry": "default",
  "connect_timeout_ms": 10000,
  "pool_idle_timeout_ms": 90000,
  "pool_max_idle_per_host": 32
}
```

A `preset` emulation is one of wreq-util's named TLS/HTTP presets. `http2`
selects whether to apply the preset's HTTP/2 settings; it is not an HTTP/2
protocol ban when false. `headers` controls preset headers. Profile and platform
use wreq-util serde names; invalid names fail when building a wreq client.

A `custom` emulation is an explicit `Fingerprint`, the shape channels use for a
captured CLI identity. Every field is optional and unset fields keep wreq's
(BoringSSL) defaults: `alpn` (`http1`/`http2`/`http3`, in offer order),
`min_tls`/`max_tls` (`tls10`..`tls13`), `cipher_list`, `curves_list` and
`sigalgs_list` in BoringSSL syntax, `preserve_tls13_cipher_list`, `grease`,
`ocsp_stapling`, `signed_cert_timestamps`, `http2` (`enable_push`,
`initial_window_size`, `initial_connection_window_size`, `max_frame_size`,
`max_header_list_size`, `header_table_size`, `max_concurrent_streams`,
`pseudo_header_order`, `settings_order`, `headers_priority`) and `headers`, an
ordered list of `[name, value]` default headers sent with their original casing.

```json
{
  "kind": "custom",
  "alpn": ["http1"],
  "min_tls": "tls12",
  "cipher_list": "TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256",
  "grease": false,
  "http2": { "enable_push": false, "initial_window_size": 2097152 }
}
```

wreq applies the complete emulation. reqwest applies custom default headers,
supported TLS versions, HTTP/1-only or HTTP/2-only selection, stream/connection
window sizes, maximum frame size and maximum header-list size. Cipher lists,
GREASE, TLS extension order and HTTP/2 ordering are skipped outside wreq. Named
wreq-util presets are wreq-only; portable presets use `kind: custom`. Explicit
request headers override fingerprint default headers. Unknown fields and
disabled backends fail explicitly. Normal certificate verification
remains enabled. Configuration is not pre-validated; backend construction errors
are returned to the caller.

`Backend::ReqwestNative` builds reqwest 0.12 exactly as the Codex CLI does: native
TLS, h2's stock SETTINGS, no decompression and no retry layer, so a profile
asking for a decoder or `retry: default` skips those unavailable options.
The native TLS API also has no TLS 1.3 min/max selector; those selectors are skipped. Its WebSocket variant (the
pool's `get_websocket`) is the rustls `reqwest` client, as the CLI's own
WebSocket is rustls; the feature therefore implies `reqwest`. On Linux OpenSSL
is built from source (`openssl-src`, so `perl` and `make` are build
requirements) and, when `wreq` is also enabled, BoringSSL is built with
prefixed symbols (`nm`/`objcopy`): the two TLS libraries share archive and
symbol names, and the system `libssl.so` cannot be linked next to wreq's
static BoringSSL. The CLI itself links the system OpenSSL on glibc; the
ClientHello of a from-source OpenSSL 3 is the same. The feature is native only.

## Reuse and lifetime

The cache key contains all effective connection parameters plus the WS HTTP/1.1
override, not profile ID, name,
version, destination or API key. Proxy URLs are parsed and normalized to authority
form, including host, default port and trailing slash spelling. Parse failures
return an error. If either idle pool parameter is zero, both are normalized to zero.
Proxy credentials
and all emulation, decompression, redirect and retry parameters participate in equality. Use API keys on individual
requests. Cookie stores and connection-bound account identity are not enabled.

Concurrent misses for the same key share one build, performed on Tokio's blocking
pool. Construction failures are not cached. Defaults retain up to 256 clients
with 10 minutes of cache idle time; each client retains up to 32 idle connections
per host for 90 seconds. These are cache/idle limits, not request concurrency limits.
Moka applies capacity/expiry maintenance on access; call `prune()` periodically
for prompt reclamation when the application is otherwise idle.

Editing parameters naturally selects a different key. Eviction/`clear()` drops
only cache ownership: existing handles and response streams remain usable.
System proxy discovery occurs in the backend at construction; treat environment/OS
proxy configuration as stable during pool use and clear the cache after changes.

Both backends default to no redirects, retries or automatic decompression; each
is configurable. `gzip`, `brotli`, `deflate` and `zstd` independently enable the
backend decoder and automatic Accept-Encoding negotiation. A preset or explicit
Accept-Encoding header takes precedence over automatic header generation. Decoding
also removes Content-Encoding/Content-Length as defined by the native backend;
false preserves encoded bytes/headers. With emulation, the explicit decoder flags
are applied after the preset.

`redirect_max_hops > 0` follows redirects up to that limit, then reports an error.
`retry = "default"` restores each backend's safe protocol-NACK retry policy
(currently at most two retries); it does not retry arbitrary HTTP errors or add a
status-code classifier. `never` disables these native retries. Application routing
attempts are a separate host policy. HTTP error status codes remain responses. There is a connect timeout but no whole-response timeout,
so long generation streams are not cut off by a client-wide deadline. Cancellation,
per-request deadlines and concurrency limits belong to the caller.

Validation: `cargo test -p gproxy-client --all-features`. Loopback tests cover TCP
reuse, HTTP proxy authentication/HTTPS CONNECT, TLS ClientHello differences between
wreq profiles, cache identity/concurrent misses, redirect limits, independent codec switches
response survival after clearing the cache, streaming multipart uploads and WS
text/binary/subprotocol/close behavior over direct and HTTP-proxy connections. ClientHello capture is not a complete
TLS/browser-fingerprint equivalence or real-provider acceptance test.
