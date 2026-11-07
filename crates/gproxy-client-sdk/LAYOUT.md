# Module layout and review map

The caller-facing SDK keeps its language IR, operation-specific IRs and native
passthroughs together. Start with `ir/` for shared content, `client.rs` for language
execution, and the selected operation family for its request/result/event models.
`lib.rs` declares modules and exports the public API; `typescript.rs` owns explicit
TypeScript export. The root `s3` path remains an alias of `assets::s3`.

- `backend/`: transport trait and the remote implementation. The in-process
  implementation is `gproxy_sdk::ClientBackend`, in the `gproxy-sdk` crate.
- `options/`: common ProviderOptions and SDK-owned language options per vendor.
- `assets/`: delivery modes/capabilities, prepared snapshots, traversal and
  validation; optional S3 support, with media client methods in `s3/media.rs`.
- `dialects/`: language render/parse/stream dispatch. Vendor folders own metadata,
  native mappings, streaming state and diagnostics. Larger handlers have children.
- `dialects/shared/`: language media/citation mappings, reasoning, sources,
  cross-vendor diagnostics and shared traversal.
- `stream/`: language events, aggregation, lifecycle and bytes-to-SSE framing.
- `embeddings/`: embedding IR, execution and OpenAI/Gemini native mappings.
- `images/`: generation/editing IR, dimensions, native options/mappings, asset
  preparation and partial-image streams.
- `speech/`: synthesis IR/options, native mappings, chunk streams and explicit
  audio aggregation. Ordinary streams retain no complete audio buffer.
- `transcription/`: ASR IR/options, typed multipart and GenerateContent mappings,
  timestamps, diarization, asset preparation and streams.
- `media_support/`: shared media wire/codec helpers, observed usage, input
  conversion and multipart encoding. This is independent of language renderers.
- `realtime/`: opt-in duplex config, sender/session/stream lifecycle, native
  OpenAI/Gemini mappers, local notifications and transcript-only persistence.
  See [REALTIME.md](REALTIME.md) for transport and interruption details.
- `provider_metadata.rs`: SDK-owned Gemini Part metadata shared by language and
  media operations, preserving signatures for same-vendor replay.
- `operations.rs` and `operations/`: feature-scoped native passthroughs, including
  the unchanged asynchronous video API.

Tests are grouped by dialect, operation, assets, stream, persistence, errors and
backend. Shared fixtures and scripted transports live in `tests/support/`.
See [MEDIA.md](MEDIA.md) for mapping choices, warnings and deferred capabilities.

## Verification

| Configuration | Passing tests |
|---|---:|
| Default features | 127 |
| s3,ts | 144 |
| realtime,s3,ts | 170 |

Counts include doctests. Realtime contributes 25 offline tests, including two
memory/custom in-process routing tests (through `gproxy_sdk::ClientBackend`, a
path-only dev-dependency), one for each dialect. Three additional IR
tests verify typed interruption persistence, the older reader's Unknown fallback,
and silent exclusion by all four chat renderers. Realtime history tests also
verify that interruption records and their metadata/cache fields are skipped.
Saved frame traces are under `tests/realtime/fixtures/`.

All 114 baseline and 127 full-feature exported TS files pass strict TypeScript
checking, including Interruption. Verification covers minimal remote/OpenAI wasm,
remote+realtime with Workers WebSocket wasm, strict Clippy for default/s3,ts/full
realtime configurations, rustfmt and the SDK-scoped diff check. Realtime
remains off by default; Interrupted is persisted independently of that feature.

These are offline checks. No paid provider smoke test is run; injected embedded
upstreams are scripted sockets, and Workers compilation does not establish live
Cloudflare deployment behavior. Review-fix logs are under
`target/client-sdk-interruption/`.

## Rust file tree

Line counts are physical lines after rustfmt. Documentation and Cargo metadata
are omitted from this Rust-only inventory.

```text
src/
├── assets/
│   ├── s3/
│   │   └── media.rs (109)
│   ├── capabilities.rs (366)
│   ├── mod.rs (58)
│   ├── prepare.rs (310)
│   ├── resolve.rs (157)
│   ├── s3.rs (345)
│   └── validation.rs (249)
├── backend/
│   ├── mod.rs (55)
│   └── remote.rs (121)
├── dialects/
│   ├── chat/
│   │   ├── render/
│   │   │   └── roles.rs (194)
│   │   ├── stream/
│   │   │   └── choice.rs (260)
│   │   ├── audio.rs (62)
│   │   ├── metadata.rs (101)
│   │   ├── mod.rs (20)
│   │   ├── parse.rs (298)
│   │   ├── render.rs (303)
│   │   ├── stream.rs (215)
│   │   └── warnings.rs (30)
│   ├── claude/
│   │   ├── parse/
│   │   │   └── content.rs (242)
│   │   ├── render/
│   │   │   └── blocks.rs (201)
│   │   ├── metadata.rs (82)
│   │   ├── mod.rs (15)
│   │   ├── parse.rs (226)
│   │   ├── render.rs (282)
│   │   ├── stream.rs (328)
│   │   └── warnings.rs (50)
│   ├── gemini/
│   │   ├── parse/
│   │   │   └── content.rs (209)
│   │   ├── render/
│   │   │   └── content.rs (166)
│   │   ├── metadata.rs (1)
│   │   ├── mod.rs (20)
│   │   ├── parse.rs (192)
│   │   ├── parts.rs (387)
│   │   ├── render.rs (302)
│   │   ├── stream.rs (283)
│   │   └── warnings.rs (71)
│   ├── openai/
│   │   ├── parse/
│   │   │   └── items.rs (276)
│   │   ├── render/
│   │   │   ├── assets.rs (187)
│   │   │   └── messages.rs (274)
│   │   ├── metadata.rs (273)
│   │   ├── mod.rs (20)
│   │   ├── parse.rs (293)
│   │   ├── render.rs (299)
│   │   ├── stream.rs (149)
│   │   ├── tools.rs (61)
│   │   └── warnings.rs (21)
│   ├── shared/
│   │   ├── citations/
│   │   │   ├── claude.rs (253)
│   │   │   ├── gemini.rs (112)
│   │   │   ├── mod.rs (112)
│   │   │   └── openai.rs (121)
│   │   ├── warnings/
│   │   │   ├── content.rs (190)
│   │   │   ├── fields.rs (164)
│   │   │   └── mod.rs (193)
│   │   ├── media.rs (282)
│   │   ├── mod.rs (17)
│   │   ├── reasoning.rs (243)
│   │   ├── sources.rs (222)
│   │   └── wire.rs (125)
│   ├── mod.rs (104)
│   └── parse.rs (24)
├── embeddings/
│   ├── client.rs (76)
│   ├── gemini.rs (240)
│   ├── mod.rs (172)
│   └── openai.rs (158)
├── images/
│   ├── client.rs (114)
│   ├── dimensions.rs (162)
│   ├── gemini.rs (219)
│   ├── mod.rs (193)
│   ├── openai.rs (283)
│   └── stream.rs (287)
├── ir/
│   ├── citation.rs (85)
│   ├── content.rs (111)
│   ├── media.rs (138)
│   ├── message.rs (57)
│   ├── mod.rs (26)
│   ├── model.rs (64)
│   ├── request.rs (102)
│   ├── result.rs (94)
│   ├── tool.rs (54)
│   └── warning.rs (31)
├── media_support/
│   ├── assets.rs (151)
│   ├── audio.rs (38)
│   ├── gemini.rs (182)
│   ├── io.rs (212)
│   ├── mod.rs (107)
│   └── multipart.rs (132)
├── operations/
│   ├── audio.rs (132)
│   ├── count_tokens.rs (126)
│   ├── json.rs (83)
│   ├── models.rs (20)
│   └── video.rs (62)
├── options/
│   ├── claude.rs (48)
│   ├── gemini.rs (36)
│   ├── mod.rs (58)
│   └── openai.rs (204)
├── realtime/
│   ├── gemini/
│   │   ├── config.rs (246)
│   │   ├── decode.rs (340)
│   │   ├── input.rs (148)
│   │   └── mod.rs (8)
│   ├── openai/
│   │   ├── audio.rs (73)
│   │   ├── config.rs (182)
│   │   ├── decode.rs (202)
│   │   ├── dispatch.rs (148)
│   │   ├── items.rs (140)
│   │   └── mod.rs (19)
│   ├── collect.rs (200)
│   ├── config.rs (94)
│   ├── events.rs (155)
│   ├── mod.rs (42)
│   ├── queue.rs (34)
│   ├── sender.rs (340)
│   ├── session.rs (240)
│   └── stream.rs (170)
├── speech/
│   ├── client.rs (102)
│   ├── gemini.rs (161)
│   ├── mod.rs (78)
│   ├── openai.rs (156)
│   ├── state.rs (227)
│   └── stream.rs (262)
├── stream/
│   ├── decoder.rs (127)
│   ├── mod.rs (192)
│   └── sse.rs (126)
├── transcription/
│   ├── client.rs (78)
│   ├── gemini.rs (246)
│   ├── mod.rs (171)
│   ├── openai.rs (366)
│   └── stream.rs (286)
├── client.rs (258)
├── error.rs (259)
├── lib.rs (83)
├── operations.rs (78)
├── provider_metadata.rs (187)
└── typescript.rs (68)

tests/
├── assets/
│   ├── capabilities.rs (85)
│   ├── client.rs (40)
│   ├── errors.rs (121)
│   ├── policy.rs (342)
│   └── preparation.rs (99)
├── errors_warnings/
│   ├── compatibility.rs (144)
│   ├── errors.rs (326)
│   ├── media_options.rs (53)
│   ├── provider_options.rs (352)
│   └── reasoning.rs (120)
├── realtime/
│   ├── config.rs (171)
│   ├── embedded.rs (143)
│   ├── gemini.rs (286)
│   ├── lifecycle.rs (290)
│   ├── openai.rs (283)
│   ├── support.rs (166)
│   └── transport.rs (208)
├── s3/
│   ├── client.rs (307)
│   ├── objects.rs (211)
│   └── policy.rs (102)
├── stream/
│   ├── aggregation.rs (278)
│   ├── edge_cases.rs (99)
│   ├── metadata.rs (148)
│   └── termination.rs (78)
├── support/
│   ├── content.rs (70)
│   ├── generation.rs (95)
│   ├── media.rs (156)
│   ├── mod.rs (196)
│   ├── native.rs (299)
│   └── s3.rs (246)
├── assets.rs (121)
├── backend_embedded.rs (141)
├── backend_remote.rs (298)
├── dialect_chat.rs (348)
├── dialect_claude.rs (245)
├── dialect_gemini.rs (394)
├── dialect_openai.rs (193)
├── embeddings.rs (392)
├── errors_warnings.rs (34)
├── images.rs (308)
├── interruption.rs (122)
├── operations.rs (275)
├── persistence.rs (195)
├── realtime.rs (16)
├── s3.rs (40)
├── speech.rs (292)
├── stream.rs (27)
└── transcription.rs (351)
```

No Rust file exceeds 400 lines. The largest source file remains
`src/dialects/gemini/parts.rs` (387 lines); the largest overall is
`tests/dialect_gemini.rs` (394 lines). Realtime source files are at most 340 lines.
