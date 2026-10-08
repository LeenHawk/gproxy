# Claude Code 2.1.294 channel audit

Updated the official native CLI from 2.1.293 through proxy `100.64.1.4:10808`
on 2026-10-08. `claude --version` reports 2.1.294. SHA-256 matches the
[official release manifest](https://downloads.claude.ai/claude-code-releases/2.1.294/manifest.json):
`27122ca7b624f537546fbef35b80c66370d974ff258f3d9b10ac50bb8771f262`.
Embedded build: `2026-10-08T02:40:59Z`, commit
`8f033c6ebe3d82a87f502e199307f38f5d55ccca`.

Extracted 2272 embedded JavaScript modules (43,419,011 source bytes) to
ignored `samples/claude-code-2.1.294/`, alongside scripts, captures and
comparison evidence. The official changelog describes fixes to prompt/agent
hook decisions, including Stop and SubagentStop hooks.

## Channel changes and evidence

- Updated CLI version, default/accepted User-Agent and audit references.
- Existing billing fixtures now expect suffix `1ea` instead of `1d1` for
  `aaaa😀 reply with exactly: ok`. Independently calculated with Node using
  the extracted salt, UTF-16 indices and SHA-256 algorithm. The captured
  inference prompt produces `042`, matching the new binary.
- Billing serializer is identical after normalizing build metadata.
- Messages remains `POST /v1/messages?beta=true`, SDK `0.128.0`, runtime
  `node` / `v26.3.0`, timeout `600`. Captured beta lists and top-level body
  key sets are unchanged. Header differences are version, generated IDs,
  content length and tool duration. Bodies also contain environment-specific
  paths, repository state and generated identity values.
- Refresh request bodies are unchanged. Manual-code token exchanges differ
  only in generated state after synthetic credentials are redacted. Login
  retains the same authorization endpoint, client ID, redirect and scopes.
- The previously audited CCH caller (1213 bytes), exclusion helper (1142
  bytes) and xxHash update/digest region (1104 bytes) are byte-identical.
- No added or removed date-suffixed feature strings were found.

## Validation boundaries

Offline `unshare -Urn` captures with synthetic credentials passed privacy-mode
inference, a two-request Read tool loop, expired-token refresh and manual-code
login. Tool-loop billing counters remain `(0, 1)`; the second request adds
`cc_prev_req`. Real subscription acceptance and browser OAuth were not tested;
the full GDB CCH oracle was not rerun.

`cargo test -p gproxy-channel --features claudecode --lib --test claudecode --locked`
passed 41 tests (14 library, 27 channel). `cargo fmt --all --check` and
`git diff --check` passed. The pre-existing Cargo.lock changes were preserved.

## Claude standard API coverage snapshot

Compared current ingress routes, protocol operations/models and channel
capabilities against the refreshed API overview and official
[Message Batches](https://platform.claude.com/docs/en/api/messages/batches),
[Skills](https://platform.claude.com/docs/en/api/skills) and
[WIF](https://platform.claude.com/docs/en/manage-claude/workload-identity-federation)
references. This is endpoint-level coverage, not an exhaustive field or
cross-dialect semantic audit.

| API | Current implementation |
| --- | --- |
| Messages, streaming, token counting, model list/retrieve | Declared ingress routes and Claude API / Claude Code channel capabilities. |
| Files | Claude models, ingress routes and file mapping/transfer infrastructure exist, but Claude API and Claude Code native capabilities and route selection do not expose file operations. Not end-to-end support through those channels. |
| Message Batches | No operation/model/standard ingress route for create, list, retrieve, cancel, delete or results. |
| Skills and skill versions | No standard operation/model/ingress route. |
| Managed Agents: agents, sessions, environments (beta) | Local upstream documents exist; no standard ingress or operation family. |
| Organization administration and usage reporting | No standard proxy surface. Claude API internally queries cost_report for quota. |
| WIF token exchange and refresh | Claude API implements API-key credentials, not a WIF exchange/refresh lifecycle. Static header configuration is not automatic WIF support. |

Primary source locations: `crates/gproxy-host-axum/src/ingress/surface.rs`,
`crates/gproxy-channel/src/channels/claudeapi/request.rs`,
`crates/gproxy-channel/src/channels/claudecode/mod.rs`,
`crates/gproxy-protocol/src/operation.rs`, and
`crates/gproxy-protocol/src/wire/claude/`.

## Subscription CLI call-site audit (2026-10-08)

This narrows the endpoint coverage snapshot above to what the subscription CLI
actually uses. Bundled SDK endpoint definitions alone are not evidence of a
subscription feature. Source excerpts, module names and character offsets are
saved in `samples/claude-code-2.1.294/evidence/subscription-api-callsites.json`.

The client factory `yN` calls `vMn` to check Anthropic authentication plus the
inference OAuth scope, then `pcs({isSubscriber: ...})` selects
`{apiKey: null, authToken: oauthAccessToken}`. This is not a routing switch for
every individual plan name. Endpoint presence does not establish server-side
entitlement for Pro, Max, Team or Enterprise.

| Feature | Subscription call path and evidence | gproxy coverage |
| --- | --- | --- |
| Inference, tool loops, side queries | `beta.messages.create`, `/v1/messages?beta=true`; observed with synthetic subscription OAuth in offline captures. | Claude Code generation/streaming channel. |
| Token counting | Business function `YOe` builds the same authenticated client and invokes `beta.messages.countTokens`; conditional on context/token counting use. | CountTokens operation. |
| Models | SDK list/retrieve and business functions exist, but this build's `ohr()` and `Phe()` both return false, disabling those dynamic discovery paths. | Supported by gproxy, but not shown to be an active requirement of this CLI build. |
| Standard Files | No `.files.upload/list/download/retrieveMetadata/delete` business calls found in extracted modules. Subscription attachment code instead calls `/api/oauth/file_upload` with OAuth; first-party, privacy and organization policy gates apply. | OAuth upload and `/api/oauth/files/{file_uuid}/content` are declared resource services already. Standard Files gap is not this attachment path. |
| Standard Skills | API SDK definitions and the Managed Agents `setupSkills` helper exist; no ordinary subscription flow calling standard skill CRUD was identified. Subscription skill sync uses `/api/oauth/organizations/{org}/skills/list-skills`, `.../skills/{id}/download`, and search uses `.../skills/search`, with organization OAuth authentication. | Declared skill resource services already, subject to Caller/Pool/Credential views and resource ownership. |
| Message Batches | Endpoint definitions in the bundled SDK; no `.batches.*` business call found. | Missing standard API, but not an evidenced subscription CLI requirement. |
| Standard Agents | SDK definitions; no ordinary subscription agent-CRUD caller identified. Local CLI subagents use inference, not automatically `/v1/agents`. | Missing standard API; not an evidenced subscription inference dependency. |
| Cloud sessions / Remote Control | `Yk` requires first-party provider and claude.ai OAuth. `/v1/code/sessions` is used, with `/v1/sessions` compatibility paths carrying `ccr-byoc-2025-07-29`. Bridge registers `/v1/environments/bridge`, polls `.../{id}/work/poll`, acknowledges/stops/heartbeats work, reconnects bridges and archives sessions. Bridge requests carry OAuth and a runner beta. | No corresponding standard ingress or Claude Code service routes. This is a real feature-specific gap; shared `/v1/sessions` and `/v1/environments` prefixes do not make this the full Managed Agents API. |
| claude.ai MCP discovery | `/v1/mcp_servers?limit=1000&include_additional_installs=true` (with legacy retry), bearer token and `mcp-servers-2025-12-04`; scope check for `user:mcp_servers`. Observed in privacy and tool-loop captures. | No corresponding route; Claude Code vendor service fallback only accepts `/api/`. |
| Public MCP registry | `/mcp-registry/v0/servers`, `auth: none`; observed in tool-loop capture. Also has `/api/directory/servers` fallback code. This is a public directory request, not subscription inference. | No route for the registry prefix. |
| Account and token lifecycle | `/api/oauth/profile`, bootstrap, usage and other account services; `/v1/oauth/token` refresh/login. | OAuth lifecycle and classified `/api/` services already exist; service views do not universally forward raw account state. |

The optional cloud/bridge and attachment/skill paths above are source findings,
not successful calls using a real paid account. Existing offline captures cover
inference, a Read loop, refresh/login and attempted MCP discovery, with synthetic
responses. They do not validate real plan permissions, file transfer, skills or
remote control end to end. WIF, organization Admin APIs and Message Batches
should not be prioritized merely because they exist in the standard API docs.

For subscription CLI compatibility, the evidence supports prioritizing MCP
discovery/registry and, if Remote Control/cloud sessions are in scope, their
separate session/bridge contract. Standard Files/Batches/Skills remain a distinct
Claude API-key channel coverage project.

## Table-driven unpacking and readable recovery

A subsequent full `.bun` graph parse supersedes the marker-scan counts above.
The serialized graph contains 2,511 files: 2,271 executable JavaScript modules
and 240 resources. The earlier 2,272 marker hits include one string in the
native runtime that is not an application module; all real application JS
modules were present, but resources and module-path metadata were missing.

All 2,271 JS modules now have original embedded paths and formatted source in
`samples/claude-code-2.1.294/readable/`. Babel parsed all modules with zero
failures; all embedded static imports resolve. The AST index records 177,040
static imports, 1,356 literal dynamic imports and 564 endpoint literals/templates.
This is structural coverage, not a proof of runtime reachability.

The resource pass decoded 138 zstd blobs and 18 UTF-16 texts and preserved
80 text files and 4 other binaries. Two browser JS resources were additionally
formatted. Original bytes/hashes remain in `unpacked/` and its manifest.
The optional graph records yielded 134 builtin bytecode entries plus string
tables, source hashes and the prelinked module graph. No source maps exist in
the file table. Full native disassembly and ELF metadata were saved for the
main executable, clipboard extension and audio extension.

The resolved dependency chain confirms that standard `setupSkills` is called
by the SDK environment worker using a work-item sessions token/environment key.
It is distinct from the subscription CLI's organization OAuth skill sync.
See `samples/claude-code-2.1.294/README.md`, `MODULES.md`, `ENDPOINTS.md` and
`SUBSCRIPTION.md` for artifact navigation, call chains and recovery limits.

Targeted native decompilation with angr 10.0.2 additionally produced approximate
C-like pseudocode for the CCH exclusion helper at `0x35f96f0` (96 recovered basic
blocks) and xxHash update at `0x251f960` (16 blocks). Code slices are located
through ELF program headers. External calls/data and original types are not
recovered; these are not compilable original sources, and no new native
execution oracle was run. Results and limitations are in the sample's
`native/README.md`. The entire Bun/JSC runtime was disassembled, not converted
to C/C++ pseudocode.

## Implemented channel follow-up

- Claude API now declares and routes native Files upload, list, metadata,
  deletion and content download. Multipart uploads and binary response headers
  are preserved. Stable Files adds no forced beta header. Multi-workspace keys
  can pin `anthropic-workspace-id` through provider headers, or explicitly allow
  caller selection with `allowed_headers`.
- Claude Code declares `GET /v1/mcp_servers` as account-restricted service data:
  the existing admin-authorized Credential view forwards it, while Caller and
  Pool receive 403 instead of exposing the shared account's connectors or
  inventing an empty list. On a provider mount, admins can select this with
  `x-gproxy-view: credential:{id}`. The discovery beta is supplied if absent;
  query parameters and upstream response fields are preserved.
- Public `GET /mcp-registry/v0/servers` and the CLI's fallback
  `GET /api/directory/servers` forward catalog responses without Authorization,
  Cookie, x-api-key or x-organization-uuid, including static header overrides.
- Batches, standard Skills and Remote Control remain subsequent extensions.

Validation: 51 Claude channel/library tests and 25 Axum router tests passed.
The public-directory credential-stripping regression was rerun after including
static cookies. Changed Rust files passed targeted rustfmt; workspace formatting
and diff checks passed. Real API calls and subscription entitlement were not
exercised by these tests.

## Live subscription scope verification

A user-supplied Max OAuth credential was tested directly through the requested
proxy. No credential values are retained in this report.

- `/api/oauth/profile` returned 200, confirming the access token was valid.
- Standard and beta `GET /v1/messages/batches?limit=1` returned 403 with a
  scope requirement of any of `user:batch`, `user:developer`,
  `workspace:developer`, `workspace:inference`.
- Standard and beta `GET /v1/skills?limit=1` returned 403 with a scope
  requirement of any of `org:skills`, `user:developer`, `user:managed_agents`,
  `user:skills`, `workspace:developer`, `workspace:skills`.
- Two refresh grants using the official Claude Code client ID and the existing
  CLI scopes plus, respectively, `user:batch` and `user:skills` both returned
  HTTP 400 `invalid_scope`: "The requested scope is invalid, unknown, or
  malformed." Request IDs: `req_011Cfpx4iU9L38Zn6Ra2Q9yD` and
  `req_011Cfpx4kfMnRJYnjvhvedwW`. No new tokens were issued or saved.

Important correction: official `chunk-kyx7hpb0.js` does contain a refresh-based
`projects_scope_expansion` flow for `user:projects:read/write`. Therefore scope
expansion on refresh must not be described as universally impossible. The live
negative finding is specifically for this credential/client and these two
requested scopes. It does not establish the outcome of interactive re-consent,
other OAuth clients or developer/workspace grants.

The public Claude Code client metadata endpoint returns redirect/grant metadata,
not a scope allowlist. Anonymous authorization-page probes for baseline and
expanded scopes all hit a Cloudflare challenge (403), not OAuth `invalid_scope`;
those page responses provide no evidence about authorization-server acceptance.
Official WIF documentation describes `workspace:developer` as workspace API-key
access, not a Claude Max subscription entitlement. Default channel scopes remain
unchanged.
