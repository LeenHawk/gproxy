# Claude Code 2.1.292 channel audit

The local official native CLI was updated from 2.1.289 to 2.1.292 with
`claude update` through HTTP proxy `100.64.1.4:10808`.
Binary SHA-256: `a967e7b1d8b4e47ee421d5433027880347952b0c0857abf880e2c942a4ec93b3`.
Embedded build: `2026-10-06T05:25:12Z`, commit
`37832d0b7cad7b40bac7c82dff58629313913edf`.

Extracted 2258 embedded JavaScript modules (43,037,305 source bytes) into
ignored `samples/claude-code-2.1.292/decompiled/`. Reproduction scripts,
module hashes, synthetic captures and comparisons are under that sample root.

## Channel changes

- Updated `CLI_VERSION` and `CLI_USER_AGENT` in
  `crates/gproxy-channel/src/channels/claudecode/mod.rs` to 2.1.292 and updated
  the audit references. The UA validator accepts only the configured version,
  so valid 2.1.292 client entrypoints are now preserved instead of being replaced
  with the 2.1.289 default.
- Updated version-dependent fixtures and billing suffix expectations in
  `crates/gproxy-channel/tests/claudecode.rs`. The suffix algorithm is unchanged;
  its version input changes. The existing surrogate-pair fixture changes from
  `58d` to `acf` (independently calculated with Node). Production suffixes are
  already computed dynamically.
- No OAuth, SDK/runtime header, beta policy or CCH algorithm change was identified
  in the inspected source and scenarios.

The channel identity and existing test expectations are aligned to this audit.

## Verified contract

Reused the previous offline HTTPS CONNECT mock, with isolated configuration and
synthetic credentials inside `unshare -Urn`. Privacy-mode inference, a Read tool
loop, expired-token refresh and manual-code login all exited successfully.
The tool loop sent two Messages requests.

- Messages remains `POST /v1/messages?beta=true`; API version `2023-06-01`.
- UA is `claude-cli/2.1.292 (external, sdk-cli)` in these headless scenarios.
- SDK remains `0.128.0`, runtime `node` / `v26.3.0`, timeout `600`, retry `0`.
- Captured beta lists and top-level body key sets match 2.1.289. Header differences
  are the UA, generated IDs, content length and tool execution duration.
- OAuth endpoints, client ID, refresh scopes and manual-login scopes match.
- Billing serializer `Gno` (module 0173) retains the previous field order,
  conditional fields and validators. The tool loop retains indices `(0, 1)`;
  its second request adds `cc_prev_req`.
- Suffix functions `Ak` / `f8o` (module 0271) retain salt `59cf53e54c78`, UTF-16
  indices `[4, 7, 20]` and three hex digits of SHA-256. The capture prompt yields
  `544`, independently reproduced with Node.
- No added or removed date-suffixed feature strings were found by comparing
  the extracted JavaScript modules; this is supporting evidence, not exhaustive
  validation of feature-gated behavior.

## Native CCH comparison

Mapped the 2.1.289 ELF LOAD segments to extract the previously audited CCH caller
(1213 bytes), exclusion helper (1142 bytes), and xxHash update/digest region
(1104 bytes). All three byte sequences occur unchanged in the new binary, at
file offsets `0x33eff9e`, `0x33f86f0`, and `0x231e960`, respectively.
Evidence: `samples/claude-code-2.1.292/evidence/native-comparison.json`.

The old 355-case GDB oracle was not rerun. This audit establishes byte equality
for those regions and observes the new CLI's synthetic wire requests; it does
not claim a new exhaustive native/Rust differential test, real subscription
acceptance, browser authorization, or TLS fingerprint equivalence.

## Channel validation

`cargo test -p gproxy-channel --features claudecode --lib --test claudecode --locked`
passed 14 library tests and 27 channel tests. `cargo fmt --all --check` and
`git diff --check` passed. Compilation emitted unused-code warnings in shared
channel helpers under this feature selection.
