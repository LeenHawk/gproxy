# Claude Code 2.1.280 channel audit

Audited on 2026-09-23 against the installed official Linux x64 CLI, SHA-256
`1e08503dbdf3c2cb0d706d32f3408277388d1c76ef108673e8fe42c1b322925b`. Embedded build: `2026-09-21T20:40:17Z`,
commit `80abbfe7d7232280011ff01a21ae3338f4c6e372`.

## Evidence

A local HTTP capture used an isolated `CLAUDE_CONFIG_DIR`, a dummy OAuth token,
`ANTHROPIC_BASE_URL=http://127.0.0.1:18768`, disabled nonessential traffic,
and `claude -p 'Reply with captured' --model claude-sonnet-4-6 --tools ''
--no-session-persistence --setting-sources '' --output-format json`.
The mock returned a Messages SSE response; the CLI completed successfully.
No production credentials or generation requests were used.

Observed request: `POST /v1/messages?beta=true`, bearer auth,
`user-agent: claude-cli/2.1.280 (external, sdk-cli)`, SDK `0.112.1`,
Stainless runtime `node` / `v26.3.0`, timeout `600`, `x-app: cli`,
`anthropic-version: 2023-06-01`, direct-browser-access `true`, and a session UUID.
`metadata.user_id` remains a JSON string with `device_id`, `account_uuid`,
and `session_id`. The preliminary `/api/hello` request reports `Bun/1.4.3`.
This HTTP capture does not establish TLS ClientHello equivalence.

The captured billing prefix was
`x-anthropic-billing-header: cc_version=2.1.280.7c4; cc_entrypoint=sdk-cli;`.
The custom base URL does not activate the first-party-only billing fragments;
those were checked in the embedded JavaScript instead.

## Embedded-code checks and resulting changes

- At binary offset 190690709, OAuth configuration sets `PLUGINS_SCOPE_REGISTERED`
  to true. `dqe()` appends `user:plugins` to the five base user scopes;
  `mTr()` adds `org:create_api_key` for interactive login; `gTr()` preserves
  only the optional project read/write scopes. Both channels now request the
  plugin scope for login and refresh even when an older credential lacks it.
  Client ID, authorization URL, token URL, redirect URL, and OAuth beta are unchanged.
- At offset 193689319, billing serialization orders fields as `cc_version`,
  `cc_entrypoint`, `cch`, `cc_workload`, `cc_is_subagent`, `cc_prev_req`,
  `cc_prompt_id`, then the new `cc_turn_origin`. Its validation is exactly
  `^[a-z][a-z_]{0,31}$`; it is optional and is not synthesized. Both channels
  preserve valid incoming origins and omit invalid ones. v3 also preserves the
  other validated optional fragments it previously discarded. v4 retains its
  existing previous-request tracking and derived prompt IDs.
- At offset 198568679, the suffix algorithm still hashes salt `59cf53e54c78`,
  UTF-16 code units 4, 7 and 20 of the first user text (missing units become
  `0`), and the CLI version, then takes the first three SHA-256 hex digits.
  The captured plain-text fixture yields `7c4`; the existing surrogate-pair
  fixture yields `b74` after the version update.
- SDK identity headers did not change. The existing OAuth beta merge forwards
  caller feature betas, including newer optional feature gates; no unconditional
  feature betas were added. Request hygiene and model policy are unchanged.
- Comparing literal OAuth/CLI route prefixes against 2.1.258 found new
  `/api/oauth/local_pairing` routes. These concern device pairing, outside the
  channel's inference/login/quota surface; this update does not add pairing or
  invoke rate-limit reset endpoints.

## Validation scope

Channel regression checks cover the versioned identity and suffix, OAuth login
and refresh scopes, valid/invalid optional billing fields, existing request
hygiene, token rotation and channel services. Local capture validates the CLI's
wire shape, not production OAuth grant eligibility or model responses.
