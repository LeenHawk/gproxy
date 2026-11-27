# Codex 0.172.0 (2026-11-27)

The GPROXY channel was at 0.171.0 and now reports 0.172.0. The clean
`samples/codex` checkout was fast-forwarded from
`1145028ae4` to `181d051417`. Stable tag `rust-v0.172.0` resolves to
`fc3036dba4`; `rust-v0.171.0` resolves to `295eca9339`.

The npm platform package `@openai/codex@0.172.0-linux-x64` is in
`samples/codex-0.172.0/` (`npm-platform.json`, `package.tgz`, `package/`).
The tarball SHA-1 matches the registry `dist.shasum`. The binary was not run;
its strings contain `0.172.0`, `api_key_spend_limit_exceeded`,
`tool_result_call_id` and `daybreak_availability`.

## Wire differences

The diff touches 813 files under `codex-rs/`, mostly TUI, app-server,
sandboxing and realtime history. The parts that reach the ChatGPT backend:

- Error classification (`codex-api/src/error.rs`,
  `parse_terminal_provider_error`). `api_key_spend_limit_exceeded`,
  `project_spend_limit_exceeded` and `organization_spend_limit_exceeded` are
  a new terminal `SpendLimitExceeded` error, both statusless and on HTTP 429.
  The last two were already terminal (`QuotaExceeded`) on 429; the API-key
  code was a `RetryLimit` in 0.171.0 and is now terminal.
- Statusless `error` stream events (`sse/responses_error.rs`,
  `parse_error_event`). `server_is_overloaded` and
  `policy_capacity_unavailable` without a status are now terminal. In 0.171.0
  they were ignored and the stream kept going. The retry delay is read from
  `error.headers`, then from the event's top-level `headers`. The cyber, bio,
  invalid-prompt and misalignment codes moved into the shared parser, so they
  are also terminal on a statusless `error` event now. Any other SSE stream
  error now ends the stream at once instead of waiting for the close.
- Request body (`codex-api/src/common.rs`, `with_tool_results`). Rows of
  `input[].internal_chat_message_metadata_passthrough.executed_tool_calls[]`
  may carry new `tool_result` (`{content:[{type:"text",text}],
  structuredContent?}`) and `tool_result_call_id` fields with Apps connector
  results. The CLI adds them only when the provider base URL is exactly
  `https://chatgpt.com/backend-api/codex` (or the staging host) with ChatGPT
  auth and no custom headers or query parameters (`core/src/client.rs`,
  `sampling_result_destination`). A CLI pointed at gproxy never sends them.
  On a 307/308 with a `Location` header the CLI resends without them.
- Model catalog (`protocol/src/openai_models/access_programs.rs`).
  `available_access_programs` gains `daybreak_availability`
  (`available`, `enrollment_required`, `unavailable_for_model`, `default`;
  unknown values read as `default`).

Not changed: headers and the `User-Agent` format, originator, OAuth client
ID and token refresh, `/backend-api` routes, rate-limit headers, the
Responses WebSocket handshake and realtime events. The new config-server OAuth
discovery (`cloud-config/src/discovery.rs`) is for enterprise config servers,
not ChatGPT login.

## Implementation

- `agent.rs`: `CLI_VERSION` and its doc comment are 0.172.0. The version also
  reaches `/models?client_version=` and the `version` header.
- `retry.rs`: `api_key_spend_limit_exceeded` joins the terminal 429 codes, so
  gproxy no longer adds `Retry-After: 0` to it. A statusless `error` event
  with `server_is_overloaded` gets `retry-after` in `error.headers` unless the
  error or the event already carries one, the same advice that
  `response.failed` overloads already get. `policy_capacity_unavailable`
  stays unadvised, as on HTTP.
- No request transform changed. `tool_result`, `tool_result_call_id` and
  `daybreak_availability` pass through the existing `rest` fields of
  `ExecutedToolCall` and `ModelAccessPrograms`. Nothing is added to requests
  that the client did not send.

## Validation

- `cargo check -p gproxy-channel --all-features -j4` passed.
- `cargo test -p gproxy-channel --all-features -j4 --test codex`: 41 passed.
- Codex unit tests in the library (`--lib channels::codex`): 18 passed.
- No tests were added.
