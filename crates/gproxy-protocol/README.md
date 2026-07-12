# gproxy-protocol

[![crates.io](https://img.shields.io/crates/v/gproxy-protocol.svg)](https://crates.io/crates/gproxy-protocol)
[![docs.rs](https://docs.rs/gproxy-protocol/badge.svg)](https://docs.rs/gproxy-protocol)
[![license](https://img.shields.io/crates/l/gproxy-protocol.svg)](https://github.com/LeenHawk/gproxy)

Wire-format types and endpoint metadata for the OpenAI, Anthropic Claude, and
Google Gemini APIs.

`gproxy-protocol` is the protocol layer used by
[GPROXY](https://github.com/LeenHawk/gproxy). It provides serializable request,
response, streaming-event, and shared operation types without coupling them to
an HTTP client or server implementation.

## Usage

```toml
[dependencies]
gproxy-protocol = "2.0.15"
```

```rust
use gproxy_protocol::{Operation, Provider};
```

Protocol-specific types are available under `gproxy_protocol::openai`,
`gproxy_protocol::claude`, and `gproxy_protocol::gemini`.

## License

Licensed under the GNU Affero General Public License, version 3 or later.
