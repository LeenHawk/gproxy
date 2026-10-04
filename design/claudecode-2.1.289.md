# Claude Code 2.1.289 channel audit

Updated the official local native CLI from 2.1.288 to 2.1.289 through
`100.64.1.4:10808`. Binary SHA-256:
`a186b99e4a9c88366cd49df2f7dad56c61fc306ef0140b19ee64b7c42a8d1348`.
Embedded build: `2026-10-03T19:21:39Z`, commit
`736d26eef42d1e3017e07e6d5bd0970b8c3a068f`.

Extracted 2169 embedded JavaScript modules (41,454,963 source bytes) under
ignored `samples/claude-code-2.1.289/decompiled/`. Extraction scripts, module
offsets/hashes and synthetic captures are retained alongside the sources.

## Verified contract

The official binary completed four offline scenarios inside `unshare -Urn`:
privacy-mode inference, a two-request Read tool loop, expired-token refresh,
and manual-code login. The harness uses isolated configuration, synthetic
credentials, a process-local CA and an HTTPS CONNECT mock with no forwarding.

- Messages remains `POST /v1/messages?beta=true`; API version `2023-06-01`,
  `x-app: cli` and dangerous-direct-browser-access remain unchanged.
- Captured UA is `claude-cli/2.1.289 (external, sdk-cli)`. The channel keeps its
  interactive `cli` default and preserves valid current-version entrypoints.
- SDK remains `0.128.0`, runtime `node` / `v26.3.0`, timeout `600`, initial
  retry count `0`. Captured beta flags and top-level request keys match 2.1.288
  in the same privacy-mode scenario.
- OAuth URLs, client ID, refresh scopes and manual-login scopes are unchanged
  (modules 0032/0132 and captured refresh/login requests).
- Billing serializer `l3r` in module 0157 retains the previous field order,
  conditional fields and validators. Both captured tool-loop requests retain
  prompt/turn indices `(0, 1)`; the second adds `cc_prev_req`.
- Suffix functions `ak` / `XFo` in module 0261 retain salt `59cf53e54c78` and
  UTF-16 indices `[4, 7, 20]`. With the new version, the existing surrogate-pair
  regression suffix becomes `58d`; the capture prompt yields `e52`, matching
  independent Node SHA-256 calculation.

Updated the channel version/UA and existing identity/billing test expectations.
Feature-dependent client hints remain subject to the existing header allowlist
and beta forwarding policy.

## Native CCH verification

Compared actual executable regions using each ELF's LOAD segment mapping.
The checksum caller (`0x35f0f9e`, 1213 bytes), exclusion helper (`0x35f96f0`,
1142 bytes), and xxHash update/digest region (`0x251f960`, 1104 bytes) are
byte-for-byte identical to the pinned 2.1.288 binary. Region hashes are in
`samples/claude-code-2.1.289/evidence/native-comparison.json`.

Re-ran the native GDB oracle against 2.1.289: 355 cases completed, comprising
one fresh official request and the same 354 synthetic edge cases. All 354
synthetic outputs and hash spans match 2.1.288 exactly. Independent libxxhash
recalculation matches all 347 hashed cases; eight cases skip hashing.
Evidence and reproduction scripts are under the ignored `native-oracle/`
subdirectory. This is instrumented execution of the official native code.
The debugger terminates the process before sending the modified request.

The existing CCH implementation and the lexical rules documented in
`claudecode-2.1.288.md` therefore remain unchanged. The prior Rust differential
comparison is not represented as a new 355-case Rust run.

## Validation boundaries

`cargo test -p gproxy-channel --features claudecode --lib --test claudecode`
passed all 41 existing tests (14 library and 27 channel tests).
`cargo fmt -p gproxy-channel -- --check` and `git diff --check` passed.

These are local synthetic checks. Real subscription acceptance, browser OAuth
authorization and TLS fingerprint equivalence were not tested.
