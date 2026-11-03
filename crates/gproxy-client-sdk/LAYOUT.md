# Module layout and review map

The caller-facing SDK keeps its language IR, operation-specific IRs and native
passthroughs together. Start with `ir/` for shared content, `client.rs` for language
execution, and the selected operation family for its request/result/event models.
`lib.rs` declares modules and exports the public API; `typescript.rs` owns explicit
TypeScript export. The root `s3` path remains an alias of `assets::s3`.

- `backend/`: transport trait and remote/embedded implementations.
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
| Default features | 114 |
| s3,ts | 131 |

Counts include doctests. The media families add 24 tests (7 image, 7 speech,
10 transcription; two asset/S3 cases require `s3`). Existing native, language,
embedding, persistence and backend suites remain passing. All 113 exported TS
files pass strict TypeScript checking. Rust verification covers full s3,ts wasm,
minimal remote/OpenAI wasm, each media feature alone on wasm, strict Clippy for
default/s3,ts and each media feature alone, plus rustfmt. These are offline tests;
they do not establish live provider, browser CORS, S3 or Workers behavior.

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
│   └── validation.rs (252)
├── backend/
│   ├── embedded.rs (112)
│   ├── mod.rs (35)
│   └── remote.rs (68)
├── dialects/
│   ├── chat/
│   │   ├── render/
│   │   │   └── roles.rs (178)
│   │   ├── stream/
│   │   │   └── choice.rs (254)
│   │   ├── audio.rs (69)
│   │   ├── metadata.rs (101)
│   │   ├── mod.rs (20)
│   │   ├── parse.rs (302)
│   │   ├── render.rs (322)
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
│   │   ├── render.rs (285)
│   │   ├── stream.rs (329)
│   │   └── warnings.rs (52)
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
│   │   ├── stream.rs (202)
│   │   └── warnings.rs (71)
│   ├── openai/
│   │   ├── parse/
│   │   │   └── items.rs (266)
│   │   ├── render/
│   │   │   ├── assets.rs (192)
│   │   │   └── messages.rs (274)
│   │   ├── metadata.rs (273)
│   │   ├── mod.rs (20)
│   │   ├── parse.rs (293)
│   │   ├── render.rs (301)
│   │   ├── stream.rs (161)
│   │   ├── tools.rs (61)
│   │   └── warnings.rs (21)
│   ├── shared/
│   │   ├── citations/
│   │   │   ├── claude.rs (253)
│   │   │   ├── gemini.rs (112)
│   │   │   ├── mod.rs (112)
│   │   │   └── openai.rs (121)
│   │   ├── warnings/
│   │   │   ├── content.rs (180)
│   │   │   ├── fields.rs (164)
│   │   │   └── mod.rs (190)
│   │   ├── media.rs (282)
│   │   ├── mod.rs (17)
│   │   ├── reasoning.rs (243)
│   │   ├── sources.rs (222)
│   │   └── wire.rs (125)
│   ├── mod.rs (103)
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
│   ├── content.rs (102)
│   ├── media.rs (138)
│   ├── message.rs (39)
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
├── speech/
│   ├── client.rs (102)
│   ├── gemini.rs (154)
│   ├── mod.rs (79)
│   ├── openai.rs (147)
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
├── error.rs (247)
├── lib.rs (81)
├── operations.rs (78)
├── provider_metadata.rs (159)
└── typescript.rs (63)

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
├── s3/
│   ├── client.rs (307)
│   ├── objects.rs (211)
│   └── policy.rs (102)
├── stream/
│   ├── aggregation.rs (245)
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
├── dialect_chat.rs (281)
├── dialect_claude.rs (245)
├── dialect_gemini.rs (375)
├── dialect_openai.rs (130)
├── embeddings.rs (392)
├── errors_warnings.rs (34)
├── images.rs (308)
├── operations.rs (275)
├── persistence.rs (194)
├── s3.rs (40)
├── speech.rs (264)
├── stream.rs (24)
└── transcription.rs (351)
```

No Rust file exceeds 400 lines. The largest source file is
`src/dialects/gemini/parts.rs` (387 lines); the largest overall is
`tests/embeddings.rs` (392 lines).
