# gproxy-channel-api

[![crates.io](https://img.shields.io/crates/v/gproxy-channel-api.svg)](https://crates.io/crates/gproxy-channel-api)
[![docs.rs](https://docs.rs/gproxy-channel-api/badge.svg)](https://docs.rs/gproxy-channel-api)
[![license](https://img.shields.io/crates/l/gproxy-channel-api.svg)](LICENSE)

Traits and types for writing your own [GPROXY](https://gproxy.leenhawk.com)
channel — an adapter that teaches GPROXY how to talk to an upstream AI
provider. You implement the adapter in a separate crate, link it into a custom
GPROXY binary, and it shows up in the console next to the built-in channels.

## What a channel does

A channel answers four questions for the GPROXY core:

- **What can it serve?** `Channel::routing_table()` declares the supported
  operations (chat completions, responses, models, ...).
- **How is a request sent upstream?** `Channel::prepare()` injects credentials
  and produces the absolute upstream URL. Optional `shape_request` /
  `shape_response` hooks rewrite bodies, and a `stream_decoder` can decode
  provider-specific streaming formats.
- **How do credentials stay valid?** `needs_refresh` / `refresh` handle silent
  OAuth renewal; the separate `ChannelLogin` trait covers first-time
  interactive login (auth-code + PKCE, device code, or cookie exchange).
- **How is it doing?** `classify()` maps upstream responses to health
  dispositions, and the usage hooks report quota / rate-limit state.

Only the upstream-facing contract lives here. Persistence, credential
encryption, proxy selection, and the HTTP server stay in the GPROXY host — your
adapter never touches them.

## Usage

```toml
[dependencies]
gproxy-channel-api = { version = "=2.3.1", features = ["external-channels"] }
linkme = "0.3"
```

Implement `Channel`, then register it with a `linkme` distributed slice so the
host binary picks it up at link time:

```rust
use std::sync::Arc;
use gproxy_channel_api::{Channel, RegisteredChannel};

pub struct MyChannel;

impl Channel for MyChannel {
    // id(), provider_family(), routing_table(), prepare(), ...
}

fn register() -> RegisteredChannel {
    RegisteredChannel::new(Arc::new(MyChannel))
}

#[linkme::distributed_slice(gproxy_channel_api::registration::CHANNEL_REGISTRATIONS)]
static REGISTER: gproxy_channel_api::ChannelRegistration = register;
```

The `external-channels` feature enables the registration slice; it is
native-only (registration is not available on `wasm32` targets).

`ByteStreamDecoder::push` and `finish` return `Result<Vec<u8>, ClientError>`.
Provider-specific decoders must return `ClientError::Decode` for malformed,
truncated, or oversized input so the host can terminate the response as failed.

A complete, runnable setup lives in the
[`examples/external-channel/`](https://github.com/LeenHawk/gproxy/tree/main/examples/external-channel)
workspace: an adapter crate, a custom binary that links it, and a test that
asserts the channel is actually registered. The
[Adding a Channel](https://gproxy.leenhawk.com/guides/adding-a-channel/) guide
walks through it step by step.

## Version matching

Match your `gproxy-channel-api` version to the GPROXY source you build the
binary from (same tag, or the same workspace when building from source). If
Cargo resolves two copies of the crate from different sources, each copy has
its own registration slice — the host only sees channels registered in *its*
copy, so your channel would silently not appear.

## License

Licensed under the [MIT License](LICENSE).
