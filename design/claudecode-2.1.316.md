# Claude Code 2.1.316 audit (2026-11-01)

## Installation and extraction

Queried official npm and ran `claude update` with process-scoped
`HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` set to
`http://100.64.1.4:10808`. The installed CLI advanced from 2.1.302 to
2.1.316; `claude --version` confirms it. No persistent proxy settings changed.
The gproxy channel previously impersonated 2.1.311.

The installed Linux-x64 binary matches the official release manifest downloaded
through the same proxy:

- SHA-256: `fd13c70000fec3f2116dfbcbb6121210b4a29328bf0e46210a59dd594a67d824`
- Commit: `cd7ceb459c6864567b16a11f60101b24421475c1`
- Manifest build: `2026-10-30T23:47:48Z`
- Embedded build: `2026-10-30T23:28:13Z`

Recovered the full Bun serialized module graph: 2,880 files, including 2,648
JavaScript modules. All JS modules parsed and formatted successfully, with no
unresolved embedded static imports. The graph contains 54,595,511 content
bytes and no embedded source maps. The shared bytecode region is saved once,
not once per module. This is embedded-source recovery and targeted native
execution/disassembly, not decompilation of every native function.

Local evidence is in ignored `samples/claude-code-2.1.316/`, including the
binary, manifest, recovered graph, readable sources, captures and comparisons.

## Wire comparison

Executed the pinned native CLI in an isolated network namespace containing
only loopback, using synthetic OAuth credentials and a local HTTPS mock.
Baseline inference, Read tool loop, expired-token refresh, privacy environment
and manual authorization-code exchange all completed successfully. The tool
loop made two Messages calls and returned a successful final CLI result.
A separate PTY run captured an interactive CLI prompt and displayed `captured`:
its main request uses `(external, cli)`, `cc_turn_origin=human` and indices
`(1,1)`. This build defaults to auto permission mode; its interactive main
request includes the conditional dangerous-tool-use/AFK betas and `safeguards`.
The channel already preserves native unknown fields and client beta headers.

Compared with the existing 2.1.311 captures:

- Messages remains `POST /v1/messages?beta=true`.
- Stainless package `0.128.0`, runtime `node` / `v26.12.1`, timeout `600`,
  default beta list and body keys are unchanged. Date-suffixed feature literal
  sets are equal across the recovered JS modules.
- OAuth endpoints, client ID, callback, scopes and token request bodies are
  unchanged, apart from per-login state/PKCE material. Profile user agent
  follows the CLI version; token exchange still uses `axios/1.15.2`.
- Captured default billing field order and tool-loop indices are unchanged;
  the second tool-loop call carries `cc_prev_req=req_audit_1`.
- CLI agent instructions change (for example, the surrounding-code sentence
  disappears); gproxy does not inject that full agent prompt.
- Native Messages still sends `anthropic-version: 2023-06-01`. The channel
  deliberately retains the repository's shared API version `2026-10-21` and
  associated sampling policy; this update does not change that policy.

## New compaction attribution

The billing builder adds three optional fields after project attribution:

- `cc_prev_msg`: `^msg_[A-Za-z0-9_]{1,64}$`.
- `cc_prev_prompt_index`: integer in `[0, 10000000]`.
- `cc_prev_turn_index`: integer in `[1, 10000000]`.

The two counters are emitted together; message ID validation is independent.
The native builder does not impose `prev_prompt_index <= prev_turn_index`.
All three require the first-party OAuth path. The main thread snapshots the
last response and its turn position at compaction, retains it across retries,
and clears it when the post-compaction turn completes or session state resets.
These are pre-compaction counters, not counts derived from the compacted
Messages array.

The channel now preserves validated client-provided values in native order.
It does not synthesize them from the current transcript or persist a guessed
last-response association. Existing direct-OAuth policy still omits gateway
and project attribution. Source validation, field order and boundary behavior
were also checked by executing the recovered builder with synthetic inputs;
this is not an end-to-end compaction capture.

Source pointers under `readable/`:

- `chunk-3b05zaw0.js:22`: `Dns` billing builder; `:110`: counter bounds.
- `chunk-ndmse9kj.js:1650`: compaction snapshot lifecycle;
  `:4375`: snapshot from the main request journal.
- `chunk-e6aww6q8.js:109370`: main-thread request receives the snapshot;
  `:113405`: successful turn cleanup.

## Suffix and native checksum

The version suffix recipe is unchanged: salt `59cf53e54c78`, JS UTF-16 units
at positions `[4,7,20]`, version, SHA-256, first three hex digits.
Executing the recovered function (`chunk-s61vbvea.js:18633`) gives `db3` for
`aaaa😀 reply with exactly: ok`, and `182` for the captured print-mode prompt.

Rediscovered the checksum machine code in 2.1.316 and executed it under GDB
on the 1,785-case corpus plus a fresh native request. All **1,786/1,786 full
output buffers** match the current Rust `cch.rs` byte-for-byte: 1,606 patch
and 180 intentionally skip. This covers the 300-byte search boundary, lexical
model/fallback/token-limit exclusions, malformed/duplicate/nested keys,
Unicode and randomized combinations. The seed `0x4d659218e32a3268` and low
20-bit xxHash64 checksum remain compatible for this finite corpus.

- Entry `0x7929561`; interception `0x7929578`; completion `0x7929905`.
- Exclusion helper `0x792ff60`; xxHash update `0x2a14ce0`, digest `0x2a14e90`.
- Evidence: `evidence/caller.asm`, `cch-verification.json`,
  `rust-verification.log`, `native-oracle/evidence/oracle-results.json`.

## Usage and resets

Selected source slices are equal to 2.1.311 after AST identifier normalization
with property names, literals and control flow retained:

| Surface | Normalized lines | Result |
| --- | ---: | --- |
| Usage fetch, auth retry and cache | 176 | Equal |
| Low-priority headers, budget and retries | 404 | Equal |
| Weekly reset eligibility and redemption | 433 | Equal |
| Cedar grant schema, selection and redemption | 363 | Equal |
| Extended-grace eligibility | 19 | Equal |

No usage/reset or slow/extended policy change is needed. Equality of these
client slices does not establish unchanged upstream experiments or account
eligibility. Source modules: `chunk-w5ndchqs.js` (usage), `chunk-hwmsct5x.js`
(slow), `chunk-zv6whf9m.js` (Cedar), `chunk-fm6h4dv7.js` (Juniper),
`chunk-e6aww6q8.js:108499` (extended). Comparisons are in `evidence/`.

## Validation and boundaries

- Channel all-feature tests: 562 passed.
- Existing billing regression expanded for compaction field ordering,
  message-ID validation, paired counters and numeric boundaries.
- Native checksum differential verification: 1,786/1,786.
- Native all-feature/all-target Clippy and WASM all-feature library Clippy:
  passed with warnings denied.
- `cargo fmt --all --check` and `git diff --check`: passed.

All protocol captures use local synthetic responses. No real subscription
inference, browser authorization or reset redemption is claimed. No upstream acceptance test or remote publication was performed.
