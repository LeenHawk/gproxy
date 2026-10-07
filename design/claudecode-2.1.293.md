# Claude Code 2.1.293 channel audit

Updated the local official native CLI from 2.1.292 through HTTP proxy
`100.64.1.4:10808`. Binary SHA-256:
`8968405e26db478af44eabc4635ab5ca557057b702a54460a59c13e1b253e978`.
Embedded build: `2026-10-07T06:36:42Z`, commit
`3abc54a9d60b4d12c627afad22d6e5f58a6199d2`.

Extracted 2272 embedded JavaScript modules (43,418,562 source bytes) under
ignored `samples/claude-code-2.1.293/`. Scripts, module hashes, captures and
comparison evidence are retained there.

## Findings and changes

- Updated the channel version, UA and audit references to 2.1.293, preserving
  valid current-version client entrypoints.
- Updated existing test fixtures. The surrogate-pair billing suffix changes
  from `acf` to `1d1`; the capture prompt yields `e34`. Both were independently
  calculated with Node. The salt, UTF-16 indices and SHA-256 algorithm remain
  unchanged (`cE` / `HJo` in the extracted source).
- Billing serializer `Kso` retains field order, conditions and validators.
- Messages remains `POST /v1/messages?beta=true`, SDK `0.128.0`, runtime
  `node` / `v26.3.0`, timeout `600`. Captured beta lists and top-level body
  key sets match 2.1.292. Header differences are version identity, generated
  IDs, content length and tool duration.
- Refresh requests match the previous capture. Manual-code token exchange
  differs only in generated state after synthetic secrets are redacted.
  The login output retains the same authorization endpoint, client ID,
  redirect URI and scopes.
- No added or removed date-suffixed feature strings were found in extracted
  JavaScript. This does not exhaustively validate feature-gated behavior.

## Verification boundaries

The official binary passed privacy-mode inference, a two-request Read tool
loop, expired-token refresh and manual-code login in `unshare -Urn`, using
isolated configuration, synthetic credentials and the local HTTPS mock.
Tool-loop billing indices remain `(0, 1)` and the second request adds
`cc_prev_req`.

The previously audited CCH caller (1213 bytes), exclusion helper (1142 bytes)
and xxHash update/digest region (1104 bytes) occur byte-for-byte unchanged in
2.1.293. See `evidence/native-comparison.json` under the sample root.
The full GDB oracle was not rerun. Real subscription acceptance, browser
authorization and TLS fingerprint equivalence were not tested.

`cargo test -p gproxy-channel --features claudecode --lib --test claudecode --locked`
passed all 41 tests (14 library, 27 channel). `cargo fmt --all --check` and
`git diff --check` passed. The pre-existing local `Cargo.lock` changes were
preserved; tests used that working-tree lockfile.
