# Codex 0.167.0 identity and wire audit (2026-11-06)

Source checkout `samples/codex` was clean and fast-forwarded through
`http://100.64.1.4:10808` from `55897425e8` to
`ddd1812a51f4aded35c3ec5446672474d5cb546b`, with tags fetched. Official npm
`latest` reports 0.167.0 (published 2026-11-06T01:46Z; `alpha` is
0.168.0-alpha.7). `rust-v0.167.0` resolves to
`086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a`; `rust-v0.166.0` is
`6ff1e1ca3b`. The release tag sits on a release branch, not on main. The
audited stable interval is 123 commits.

## Ownership and changes

The channel identity advances from 0.166.0 to 0.167.0. `CLI_VERSION` is the
only code change. Its existing consumers update User-Agent generation,
synthesized `version` metadata and default model-catalog `client_version`
together.

Responses URLs, the WebSocket beta, originator/User-Agent format, OAuth
request parameters (client id, issuer-derived `/oauth/token`, device-code
paths), `/wham/usage` and rate-limit header names have no change in the
stable interval.

The 0.166.0 audit already added two items as current-main compatibility.
Both now ship in stable 0.167.0, and the existing protocol DTOs already match
them:

- `ContentItem::InputAudio` flattens `AudioReference::{Inline{audio_url},
  File{file_id}}` (`protocol/src/models.rs`, `user_input.rs`). Upstream
  converts a file-referenced audio item to a function-call output as text.
  The tool-result audio variant stays inline-only. The
  `wire/openai/codex/client.rs` `AudioReference` and `ContentItemInputAudio`
  already match this.
- `ModelMessages.apps.usage_instructions` (`openai_models.rs`) already exists
  in `wire/openai/models.rs` as `AppsMessages`.

`ModelInfo.used_fallback_model_metadata` is removed upstream, and a runtime
`CatalogModel`/`ModelOrigin` wrapper replaces it. The field was
`skip_serializing, skip_deserializing`, so it was never on the wire, and
GProxy has no copy of it.

## Client-side only (no channel change)

- `codex-api/src/error.rs` classifies 400 terminal codes as
  `InvalidAudioInput`: `audio_too_large`, `audio_too_short`, `invalid_audio`,
  `invalid_audio_data`, `invalid_audio_format` and
  `missing_audio_sample_rate`. GProxy passes upstream errors through
  unchanged.
- On `response.incomplete`, the CLI copies output items whose `status` is
  `incomplete` into the error for its inference traces. GProxy keeps the
  terminal event as it arrives.
- The WebSocket task now releases its connection lock and drains when the
  event consumer drops. Dropped metadata events no longer abort the stream.
  This is transport bookkeeping and does not change frames.
- The realtime transcript bookkeeping drops frame-based truncation. No
  realtime wire event is added.
- The web-search extension moves `alpha/search` onto workspace routing. On
  routed workspaces this means the backend origin plus
  `x-openai-account-routing-override` and rejected redirects. Search failures
  now go back to the model as `success: false` output instead of a fatal
  error. GProxy does not implement workspace routing for Responses either.
  The routing override depends on the account, and GProxy owns account
  selection, so the channel does not add or forward the override.
- Terminal detection adds Rex (`TERM_PROGRAM=rex`, `TERM=xterm-rex`). The
  channel's simplified fallback already emits the raw `TERM_PROGRAM` value.
- The CLI now caches personal-access-token auth, adds a credential-storage
  commit/change state, and adds the `cyber_access_programs` provider
  capability (default on for the OpenAI provider id). It also changes the
  default memory models to `gpt-6-luna`/`gpt-6.1-terra`. All of this is CLI
  configuration or credential lifecycle. The channel uses ChatGPT OAuth and
  already round-trips explicit `access_programs`.
- New MCP `_meta` keys `x-codex-root-turn-id` and
  `x-codex-cyber-access-program` go to MCP tool servers, not to Responses.

## Main beyond 0.167.0 (not adopted)

`rust-v0.167.0..main` (265 commits) adds these; none ships in a stable
release yet:

- `prompt_cache_options: {ttl: "30m" | "12h"}` on Responses requests
  (`codex-api/src/common.rs`, 41cbd5b9b2). Adopted ahead of a release:
  native CLI requests already pass it through unshaped; request shaping for
  other clients now keeps a client's `ttl` and drops the public API's mode,
  prewarm and comparison options (`cli_prompt_cache_options` in
  `channels/codex/shape/responses.rs`). Backend acceptance is unverified.
- A read-only `content_token_count` on sampled messages
  (`skip_serializing`, never replayed).
- A `budget_reached` terminal code (`SessionBudgetExceeded`), and a
  `per_recording` flag on `InvalidAudioInput`.
- Catalog `web_search_provider` (`standalone` | `search_mcp`) and
  `ModelMessages.plugins` (plugin wording).
- Large auth/provider refactors: workload identity, a device OAuth module,
  credential export, and redirect-rejecting route clients.
- WebSocket telemetry changes.

## Preserved behavior

No new channel or config key, route, header, default field or model
availability claim is introduced. No defaults are added for fields the
client did not send.

## Validation

- All-feature channel `codex` test target: 41 passed.
- `gproxy-channel` all-feature all-target Clippy with warnings denied: passed.
- Protocol crate untouched (stable 0.167.0 wire already matched), so no protocol suite rerun.
- No live subscription, OAuth login, upstream inference or installed CLI
  update was performed. The public OpenAI docs and
  `upstream_docs/codex/protocol.md` were not refreshed for this bump.
