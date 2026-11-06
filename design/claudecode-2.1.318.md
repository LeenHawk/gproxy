# Claude Code 2.1.318 audit (2026-11-06)

## Download and extraction

Queried official npm and `https://downloads.claude.ai/claude-code-releases/`
with process-scoped `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` set to
`http://100.64.1.4:10808`: `@anthropic-ai/claude-code` latest `2.1.318`,
`@anthropic-ai/claude-agent-sdk` latest `0.3.318` (`claudeCodeVersion`
`2.1.318`), release `latest` `2.1.318`, `stable` `2.1.311`. Downloaded the
Linux-x64 binary and release manifest into ignored
`samples/claude-code-2.1.318/`. The installed CLI and persistent proxy
settings were not changed. The channel previously impersonated 2.1.316.

- SHA-256: `708c77b204db30086ff6f1ed34577bda03e1be74ced80557cbf7b54249ebf0bd`
  (matches the manifest; size 337,814,032)
- Commit: `ca1fa9dea62c1027756786c841c548788644560d`
- Manifest build: `2026-11-03T19:21:35Z`; embedded build `2026-11-03T18:52:53Z`
- Runtime: Bun 1.4.4, but a new Bun build (`424a542a9`, was `71c9bf0ca`)

Recovered the Bun serialized module graph: 3,100 files, including 2,857
JavaScript modules. All JS modules parsed and formatted, with no unresolved
embedded static imports, and no embedded source maps. The shared bytecode
region is saved once. The research scripts are the 2.1.316 ones, with the
Babel paths updated for the refreshed console dependencies.

## Wire comparison with 2.1.316

Ran the 2.1.316 offline harness (`research/capture.py`: a network namespace
containing only loopback, synthetic OAuth credentials and a local HTTPS mock)
for baseline, Read tool loop, expired-token refresh, privacy env, manual login
and an interactive PTY prompt. All completed successfully. The tool loop made
two Messages calls. The interactive prompt displayed `captured`, and its main
request again uses `(external, cli)`, `cc_turn_origin=human` and indices
`(1,1)`. `research/compare.py` pairs requests by method, host and path, then
normalizes versions, UUIDs, `cch` and the suffix
(`evidence/wire-comparison.json`):

- The set of requests is unchanged in every scenario. Messages remains
  `POST /v1/messages?beta=true`.
- Stainless package `0.128.0`, runtime `node` / `v26.12.1`, OS, arch,
  timeout `600`, `x-app: cli` and `anthropic-version: 2023-06-01` are
  unchanged. Body keys, `metadata.user_id`, thinking, context management,
  output config, diagnostics and `max_tokens` are equal. Identity system
  blocks are unchanged. The tool list is equal.
- Billing field order is unchanged in every capture, and the second
  tool-loop call still carries `cc_prev_req=req_audit_1`.
- OAuth authorize URL, client ID, redirect URI, scopes and token request
  bodies are unchanged, apart from per-login state/PKCE material. Token calls
  still use `axios/1.15.2`. Profile and roles follow the CLI version
  (`claude-code/2.1.318`).
- Other differences come from the environment (date, paths, the git status
  of the working tree) or are client-side and absent from the channel: two more
  `safeguards` deny rules, MCP client capabilities, and telemetry events.

### New default beta: `tool-provenance-2026-10-26`

Every first-party OAuth Messages call, main-loop or side call, now carries
`tool-provenance-2026-10-26` after `mid-conversation-tool-changes-2026-07-01`.
It is `ccr_tool_provenance` (`chunk-7kayq7nc.js:5619`), and it is gated by
`m$`: first-party, OAuth, not CCR, and flag `tengu_snappy_noodle`, which
defaults to on (`chunk-exfmq47h.js:22446`). The beta is latched per
conversation and dropped after a server refusal
(`chunk-5xgkngpb.js:103426`). With MCP tools, the tools may carry
`ownership_metadata` / `server_url`. The captures had no MCP tools, so the
body did not change.

Channel: `MAIN_LOOP_BETAS`, the list sent for a Messages call whose client
sent no beta list of its own, now ends with `tool-provenance-2026-10-26`,
keeping its native position after the always-on five. `per-turn-control` and
`mid-conversation-tool-changes` stay omitted. A native client's beta list is
forwarded as sent.

### `x-claude-code-sandy-plover` now on first-party Messages

The 2.1.311 audit recorded this header as Vertex-only. In 2.1.316 the
first-party path existed but was also gated by `tengu_snug_mitten` (default
off). 2.1.318 drops that gate. The header is now set on every first-party OAuth
`/v1/messages` call when `tengu_silly_shore` holds (default on;
`chunk-vn3g7zt8.js:14087`). It is inserted after `x-claude-code-request-class`.
Its value is a compact JSON array naming the tools used so far in the
conversation: `[]` on the first call, then `[{"name":"Read"}]` on the
tool-loop continuation. Entries may add `url`, `directory_entry` or
`unresolved`. The encoder (`Dy`, `:11785`) caps the header at 64 entries and
7,168 bytes, with names of at most 256 bytes; when entries are dropped, the
list starts with an overflow marker entry. The list comes from a
per-conversation latch (`toolsUsed`, `chunk-5ythzcas.js:700`) that tool
execution, sub-agents and handbacks feed. It is not derived from the request
body.

Channel: no change. The header is not in `CHANNEL_HEADERS`, so a native CLI
client's value is forwarded unchanged. The channel does not synthesize it for
clients that did not send it. That would mean inventing per-conversation state
the client never sent.

### Side-query and program use case betas

- `claude-code-side-query-2026-09-29` (`chunk-7kayq7nc.js:5635`) is added
  to first-party OAuth side calls such as `generate_session_title`
  (`chunk-5xgkngpb.js:112023`). The interactive title call carries it. The
  channel forwards it.
- `program-use-case-ids-2026-09-17` is added only when the user has chosen a
  program use case. That choice also adds body field `program_use_case_id`.
  This was not captured. The channel preserves unknown body fields and client
  betas.

## Billing builder

The normalized billing builder (`chunk-h5e9c4sr.js`, was
`chunk-3b05zaw0.js`) differs from 2.1.316 only in build constants and the
project ID pattern (`evidence/billing-comparison.diff`):

```
2.1.316: /^claude_proj_(?:staging_|local_)?01[1-9A-HJ-NP-Za-km-z]{22}$/
2.1.318: /^(?:claude_proj|chan)_(?:staging_|local_)?01[1-9A-HJ-NP-Za-km-z]{22}$/
```

The CLI therefore accepts `chan_…` project IDs for `cc_project`, and the same
pattern appears in the attribution settings parser (`chunk-n6r4p63b.js:67`).
Field order, `cc_project_src` sources, compaction fields and numeric bounds are
unchanged. Channel: `hygiene::is_project_id` now accepts the `chan_` prefix.
A client's validated `cc_project` / `cc_project_src` pair is still only
preserved, never synthesized.

## Suffix and checksum

The version suffix recipe is unchanged: salt `59cf53e54c78`
(`chunk-7jxyna5q.js:15159`), JS UTF-16 units `[4,7,20]`, version, SHA-256,
first three hex digits. Running the recovered function (`Du`) gives `f3f` for
the print-mode prompt and `1e6` for the interactive prompt, both as captured.
It gives `886` for the test fixture `aaaa😀 reply with exactly: ok`; the lone
surrogate encodes as U+FFFD (`evidence/suffix-execution.txt`).

`cch`: an independent Python port of `cch.rs` (`research/cch_port.py`,
pure-Python xxHash64, seed `0x4d659218e32a3268`, low 20 bits, lexical
exclusions) restores the placeholder in each captured body and recomputes it.
**7/7 2.1.318 Messages bodies match** (the title side call has no `system`
placeholder), and the same port reproduces 7/7 for 2.1.316
(`evidence/cch-captures.json`). The seed immediate is at `0x311c9b6` (was
`0x7929682`). The caller was recompiled with the new Bun build (different
register allocation and layout; `evidence/caller-31{6,8}.norm.asm`), so the
2.1.316 GDB oracle addresses do not apply. The 1,786-case native differential
corpus was **not** re-run for 2.1.318.

## Usage and resets

Reproduced the 2.1.316 identifier-normalized slices exactly from 2.1.316
sources, as runs of top-level statements (`research/slices.cjs`), then
extracted the same runs from 2.1.318:

| Surface | Normalized lines | Result |
| --- | ---: | --- |
| Usage fetch, auth retry and cache (`chunk-7pwjpdkw.js`) | 176 | Equal |
| Low-priority headers, budget and retries (`chunk-vn3g7zt8.js`) | 404 | Equal |
| Weekly reset eligibility and redemption (`chunk-sp3m4m72.js`) | 433 | Equal |
| Cedar grant schema, selection and redemption (`chunk-0467wazv.js`) | 363 | Equal |
| Extended-grace eligibility (`chunk-5xgkngpb.js:110423`) | 19 | Equal |

No usage, quota, reset or slow/extended policy change is needed. Equal client
slices do not show that upstream experiments or account eligibility are
unchanged.

## Channel changes

- `CLI_VERSION`, `CLI_USER_AGENT`, `AGENT_SDK_VERSION` (`0.3.318`),
  `AGENT_SDK_USER_AGENT`, `PROFILE_USER_AGENT` → 2.1.318; version-citing doc
  comments follow. Minified symbol names cited from earlier builds keep their
  version.
- `hygiene::is_project_id` accepts `chan_` project IDs.
- Test fixtures: user agents → 2.1.318; the fixture prompt's suffix → `886`.

## Validation and boundaries

- `cargo test -p gproxy-channel --all-features -j4 --test claudecode`:
  29 passed; `--lib claudecode`: 5 passed.
- `cargo clippy -p gproxy-channel --all-features -j4`: passed without
  warnings; `cargo fmt -p gproxy-channel --check`: clean.

All protocol captures use local synthetic responses. No real credential,
subscription inference, browser authorization, reset redemption or upstream
acceptance test was used or performed. Not verified: the native `cch`
differential corpus on the recompiled caller, the MCP `ownership_metadata`
body shape, and a `program_use_case_id` capture.
