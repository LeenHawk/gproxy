---
title: Custom Channels
description: Use your own upstream channels with GPROXY and understand the capability boundaries.
---

This page explains how to connect your own upstream channel with no code changes.

## Use your own channel (no code changes)

### Recommended scope

This mode is designed for upstreams that are already compatible with **standard** protocol shapes:

- OpenAI-compatible endpoints
- Claude-compatible endpoints
- Gemini-compatible endpoints

In practice, the custom adapter maps requests to standard paths like `/v1/...` and `/v1beta/...` with fixed auth header conventions (`Bearer`, `x-api-key`, `x-goog-api-key`).

If your upstream has non-standard signing, non-standard auth handshake, or heavily customized request/response schemas, this mode is usually not enough.

### Minimal config example

```toml
[[channels]]
id = "mycustom"
enabled = true

[channels.settings]
base_url = "https://api.example.com"

[[channels.credentials]]
id = "mycustom-main"
label = "primary"
secret = "custom-provider-api-key"
```

### Optional `mask_table` (request body field stripping)

You can remove some request fields before sending upstream:

```toml
[channels.settings.mask_table]
rules = [
  { method = "POST", path = "/v1/chat/completions", remove_fields = ["metadata"] },
  { method = "POST", path = "/v1/responses", remove_fields = ["metadata"] },
]
```

What `mask_table` can do:

- Match by HTTP method + path (supports prefix `*` match).
- Remove JSON fields by path from request body.

What `mask_table` cannot do:

- Cannot rewrite response body.
- Cannot inject custom signing logic.
- Cannot implement arbitrary protocol conversion logic.

### Capability boundary in custom mode

Custom mode can choose route behavior using `dispatch` (`Passthrough` / `TransformTo` / `Local` / `Unsupported`), but only within GPROXY's **existing** operation and protocol model.

So this mode is good for plugging in standard-compatible upstreams quickly, but not for introducing a brand-new wire protocol or bespoke conversion pipeline.

If you need to add a new native channel implementation, see the contribution section in [Development and Testing](/reference/development/).
