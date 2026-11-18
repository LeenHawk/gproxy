# AI Studio Playground wire audit

Source: our Chrome 157.0.8103.78 capture on 2026-11-17 (Asia/Shanghai),
using `https://aistudio.google.com/prompts/new_chat` with a signed-in Pro
account. This channel is not the game-creation site `playground.google` and
is distinct from the API-key `aistudio` channel.

The first-party MakerSuite bundle was:

- Build: `boq-makersuite.MakerSuite.en.KvpBpnk2vKA.2018.O`, module `_b`.
- Origin: `https://www.gstatic.com/_/mss/boq-makersuite/_/js/`.
- SHA-256: `235aa39aa1045a1f4d3c1a175544b5c478eab52d49ec1322bb6c36955ce83d69`.

Raw browser captures stay outside the repository because they contain Google
session cookies, authorization hashes, proof tokens and account details.
The fixtures in `crates/gproxy-channel/tests/fixtures/playground/` contain
only protocol examples, with proofs, thought signatures and response IDs
replaced. The model metadata is public catalog information.

## Exchanges observed

All MakerSuite methods use POST to
`https://alkalimakersuite-pa.clients6.google.com/$rpc/google.internal.alkali.applications.makersuite.v1.MakerSuiteService/<method>`.

| Method | Request | Observed result |
| --- | --- | --- |
| `ListModels` | `[]` | HTTP 200, 41 models in the outer array's first field |
| `GetAiStudioBenefitTier` | `[]` | HTTP 200, `[1]` for the signed-in Pro account |
| `GenerateContent` | JSPB array | HTTP 200, `GPROXY_CAPTURE_OK`, then a terminal signature/usage frame |
| `GenerateContent` with functions | JSPB array | Function declarations captured; the browser received HTTP 403. This capture alone does not validate function-call execution. |
| WAA `Create` | First-party request to `waa-pa.clients6.google.com/$rpc/google.internal.waa.v1.Waa/Create` | HTTP 200 |

The first-party `nDa` function maps benefit enum 3 to `TIER0`, 1 to `TIER1`,
and 2 to `TIER2`. The browser's successful generation sent `TIER1`.

## Credential boundary

The browser sends Google session cookies plus `Authorization` containing
`SAPISIDHASH`, `SAPISID1PHASH`, and `SAPISID3PHASH`. Each is a timestamp and
SHA-1 of `timestamp + " " + cookie_value + " " + origin`, where the origin
is `https://aistudio.google.com`. The signing cookies are `SAPISID`,
`__Secure-1PAPISID`, and `__Secure-3PAPISID`.

Replaying `GetAiStudioBenefitTier` with only the cookies readable through
`document.cookie` returned `SESSION_COOKIE_INVALID`; the same exchange with
the complete captured Cookie header returned HTTP 200 and `[1]`. A plain
bookmarklet therefore is not the implemented credential importer. Import
Cookie-Editor JSON/Header, including HttpOnly session cookies.

`auth_user` selects the Google identity in a multi-login cookie jar, and is
sent as `X-Goog-AuthUser`. It defaults to `0`; it is not an account ID.
The public MakerSuite browser API key was recorded in the first-party RPC
request. It differs from the WAA-only key in `dB.getMetadata`; using the WAA
key for MakerSuite yields `API_KEY_SERVICE_BLOCKED`. Neither is the user's
Gemini API key.

The credential refresh rotates the session, validates it and discovers its
benefit tier. It first posts the placeholder body `[000,"-0000000000000000000"]`
to `https://accounts.google.com/RotateCookies` with the session cookies, as
the browser's `RotateCookiesPage` frame does, and takes `__Secure-1PSIDTS` /
`__Secure-3PSIDTS` from the reply's `Set-Cookie`. A failed rotation is not
fatal; `GetAiStudioBenefitTier` then decides whether the session is alive.
The credential's expiry is the next rotation time, three hours later, not a
claim about Google's session lifetime. Google answers 429 to rotations less
than a minute apart. The rotation request follows the widely used web-client
form and has not been captured from our own browser session.

Between refreshes the `response_update` hook saves cookies Google replaces:
`Set-Cookie` on MakerSuite responses, and the rotating cookies (`*SIDTS`,
`*SIDCC`) in the Cookie header sent after the signer refreshed them. Identity
cookies are never taken from the signer, deletions are ignored, and the
secret's `retired` map holds each cookie's previous value so a request still
in flight during a rotation cannot write the old value back. A session that
Google has revoked still requires fresh Cookie-Editor data.

## Generation fields (zero-based JSPB indexes)

| Request index | Meaning |
| --- | --- |
| 0 | `models/<model>` |
| 1 | Contents: each is `[parts, role]` |
| 2 | Safety settings |
| 3 | Generation config |
| 4 | Fresh WAA proof |
| 5 | System instruction, shaped as Content |
| 6 | Tools |
| 11 | Optional browser continuation token; not copied from captured requests |
| 13 | Request context, including `[[null, null, timezone]]` |

The first-party `Tu`, `Eu`, `Fu`, `Cu`, `Bu`, and `hw` definitions establish
these Part fields:

| Part index | Meaning |
| --- | --- |
| 1 | Text |
| 2 | Inline bytes `[mime_type, base64_data]` |
| 5 | Web file reference (not implemented; use inline bytes) |
| 7 | Executable code `[language, code]` |
| 8 | Code result `[outcome, output]` |
| 10 | Function call `[name, protobuf_struct_arguments, optional_id]` |
| 11 | Function response `[name, protobuf_struct_response, optional_id]` |
| 12 | Thought flag |
| 14 | Thought signature |

Use these current first-party positions, not older adapters' Part indexes.
The `el`/`hl`/`kl` definitions and `DVa`/`CVa` functions define protobuf Struct,
Value and ListValue. Values use slots 0 through 5 for null, number, string,
boolean, Struct and ListValue, respectively. Tool function declarations are
at tool index 1; each declaration has name, description and Schema in slots
0, 1 and 2. The captured `function-tools.json` checks the schema mapping.

The first-party `cB` and `en` functions concatenate content parts with a
single space, using text, inline-data base64 or a web file ID; other part
kinds contribute an empty string. Only contents participate, not the system
instruction. The result is SHA-256 hashed for the WAA proof binding. A saved
proof is therefore not a credential and cannot substitute for signing each
request.

## Responses and streaming

The success envelope is `[[message, message, ...]]`, optionally followed by
an RPC status field. The decoder must emit individual messages before the
outer arrays close. A nonzero status trailer and an unfinished envelope are
errors, including after HTTP 200.

A message contains candidates at index 0, usage at index 2, and response ID
at index 7. Candidate content is at index 0 and finish reason at index 1.
Usage fields observed: prompt tokens 0, candidate tokens 1, total tokens 2,
and thoughts tokens 9. Usage is cumulative: retain the last snapshot rather
than summing chunks. The captured final snapshot is 227 prompt tokens,
10 candidate tokens, 68 thoughts tokens, 305 total tokens.

The successful response ended with an empty text part carrying a thought
signature. Keep that part when collecting non-stream responses and when
sending history back to Google.

The captured permission error body was `[,[7,"The caller does not have permission"]]`.
JSPB permits omitted array entries; normal JSON parsing alone rejects it.
The channel translates RPC errors to Gemini's JSON error shape, preserving
HTTP status. A 403 is not automatically a permanently invalid cookie.

## WAA runtime boundary

The Rust channel sends its already-encoded contents to a provider-selected
`POST <proof_server_url>/v1/proof/mint`, then sends the actual generation
request directly to Google through the assigned client. This is a proof
service contract, not an `aistudio-api` generation relay.

The service receives `{cookies, authUser, g1Tier, model, prompt, wireContents}`
and returns `{ok: true, proof, headers}`. The protocol is compatible with the
local `waa-signer` in `DeconstructedCube/aistudio-api`, which embeds the
SpiderMonkey-based runtime from `Mag1cFall/AIStudio2API`. There is no automatic
fallback to a third-party signing server. The configured signer must be
reachable through the provider's assigned outbound transport/proxy.

Live native validation is recorded separately from the browser captures;
unit fixtures prove protocol translation, not current Google availability.

## Native validation on 2026-11-18

A disposable Rust probe invoked this channel with the real host-assigned
`gproxy-client` transport, the imported session, and a local SpiderMonkey
signer listening on loopback. Both Google requests and the signer's Google
traffic used the same US WARP exit. No public proof service was used.

- Credential validation: HTTP 200; subscription metadata `Pro` / `TIER1`.
- Text streaming: HTTP 200; `GPROXY_NATIVE_` and `OK` arrived as separate text
  chunks, followed by `STOP`, a thought signature and final usage. Combined
  text was `GPROXY_NATIVE_OK`; final usage was 41 prompt, 10 candidate,
  33 thought and 84 total tokens.
- Function call: HTTP 200; `gproxy_capture_sum` with arguments `{a: 2, b: 3}`,
  a call ID, a thought signature and a terminal frame.
- Tool-result continuation: the complete returned model parts were sent back
  with the same call ID and `{sum: 5}`. HTTP 200; the model answered that the
  result was `5`.
- Model directory: HTTP 200; 34 entries whose upstream method list includes
  `generateContent`. The exposed method list includes generation and local `countTokens`;
  upstream-only batch/Live methods are not advertised.

The signer's page load rotated `SIDCC`, `__Secure-1PSIDCC`, and
`__Secure-3PSIDCC`. The generation RPC uses the signer's updated Cookie header
and visit/attestation headers while retaining the credential's selected
`auth_user`. The timestamped SAPISID authorization is recomputed locally.

This establishes text, streaming, models and a tool round trip. Image/audio
outputs and subscription tiers other than this Pro account were not tested
against Google. The earlier browser 403 remains a captured observation, not
proof that the account or model is unusable.

## Local CountTokens

`POST /v1beta/models/{model}:countTokens` uses Core's existing local route
and configured tokenizer. It does not contact Google or the WAA signer and
does not require a Google credential refresh. `CountTokens` remains absent
from the channel's upstream dialects so Core can select this local fallback.
The model directory advertises `countTokens` alongside `generateContent`.

The count follows the gateway's tokenizer configuration and fallback rules;
it is a local estimate, not Google's official token count. Standard Gemini
`contents` and embedded `generateContentRequest` inputs use the existing
Core extractor. Claude/OpenAI counting requests use the corresponding local
response shapes through the same route.
