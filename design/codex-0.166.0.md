# Codex 0.166.0 identity and current-main wire audit (2026-11-01)

Source checkout `samples/codex` was clean and fast-forwarded through
`http://100.64.1.4:10808` from `a6839a4812` to
`67f0994b532b8740f14b5b738af7b7b2a915b1d6`. Official npm latest reports
0.166.0; `rust-v0.166.0` resolves to
`6ff1e1ca3b30fc8b914720681fcf5fe62e95f856`. The checkout remains on main;
main is not equivalent to the stable release.

## Ownership and changes

The channel identity advances from 0.163.0 to 0.166.0; this updates its agent
and default model-catalog `client_version`. Responses URLs, WebSocket beta,
OAuth request parameters, quota paths and CLI header names require no change
in the audited range. Local Codex CLI installation was not changed.

Wire DTO changes remain in the protocol crate:

- The shared local model-directory `Model` gains typed catalog presentation
  metadata: Memento history/notes/context-management, incremental tool notices,
  Apps text and approval/permission wording. Existing local model metadata
  owns these extensions; they are not new public OpenAI `/v1/models` fields
  or authorization policy. The Console's existing metadata merge retains the
  nested values without executing or interpreting them.
- Codex-native search declares `literature_search_query`. Native channel
  forwarding retains it; the existing cross-provider search conversion carries
  it to available search/page tools with primary-source guidance. This is not
  a field of a public Responses request.
- Codex-native `input_audio` supports inline `{audio_url}` and uploaded
  `{file_id}` message references under `wire/openai/codex`. Public Responses
  input-content types remain separate. The tool-result audio variant remains
  inline-only, matching upstream. Flattened-union deserialization explicitly
  consumes the reference key so it is not duplicated in extension fields.
- Cross-dialect compaction still needs actual audio content. An unresolved
  uploaded file reference fails before sending; no audio is silently omitted.

The Apps catalog field and uploaded message-audio variant are current-main
compatibility additions absent from stable 0.166.0. Presence in source does
not establish account eligibility or released-client behavior.

## Public API documentation

Refreshed 34 official OpenAI Markdown pages through the supplied proxy;
33 changed and Using GPT-6 was identical. Public references live under
`upstream_docs/openai`, while source-backed Codex-private evidence lives under
`upstream_docs/codex/protocol.md`. The docs are a separate Git repository.
New public usage/image/Realtime fields are recorded there; this Codex update
does not present private client catalog fields as public API additions.

## Preserved behavior

Native `x-codex-*` headers already carry private Guardian metadata and the
native SSE paths retain unknown metadata events. New parent routing affinity
is transport state, not a DTO field to synthesize. Account switching and
credential ownership/storage checks belong to the CLI or existing host
credential lifecycle; no channel-owned account selector was added.

Public API support for `prompt_cache_options` still does not establish
ChatGPT backend support; existing removal during Codex request shaping stays.
No new channel/config key, route, model availability claim, quota-reset action
or live upstream acceptance claim is introduced.

## Validation

- Protocol focused suites: 23 passed, covering model-message declared
  projection and literal text, approval-key aliasing, native search/audio
  round trips, search conversion and compaction media failures.
- All-feature channel suite: 562 passed.
- Protocol/channel all-feature native all-target and WASM library Clippy:
  passed with warnings denied.
- Formatting and whitespace checks in both repositories: passed.
- Official docs: all 34 installed snapshots match their downloaded SHA-256.

No live subscription, Guardian/account eligibility, upstream inference or
publication was tested. Extracted upstream docs and source audits are local.
