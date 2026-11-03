# Module layout and review map

This is a structural refactor of the SDK at `305df46f8`. It changes file boundaries
and extracts named handlers; request/response behavior, defaults, serialization,
features and crate-root public exports stay the same. The root `s3` module remains
an alias of `assets::s3` for existing callers.

Start with `ir/` for the data model, `client.rs` for request execution, and the
chosen `dialects/<vendor>/` for native mapping. `lib.rs` only declares modules and
re-exports the existing root API. `typescript.rs` owns the unchanged export entry.

- `backend/`: the transport trait and its remote/embedded implementations.
- `options/`: common ProviderOptions plus SDK-owned native options per vendor.
- `assets/`: delivery mode/table, public preparation snapshots, traversal,
  validation and optional S3. `prepare.rs` and `validation.rs` keep `resolve.rs`
  focused on its API.
- `dialects/`: render/parse/stream dispatch. Vendor folders own their metadata,
  rendering, parsing, streaming state and request diagnostics. Larger block/role
  handlers live beneath render/parse/stream rather than in one vendor file.
- `dialects/shared/`: common wire helpers, media/citation mappings, reasoning,
  sources and cross-vendor diagnostics. Vendor-specific diagnostics stay with
  their dialect; shared traversal and field rules stay here.
- `stream/`: events, aggregation, common event lifecycle and bytes-to-SSE framing.
- `embeddings/`: IR in mod.rs, execution helpers in client.rs, native vendor code
  in openai.rs/gemini.rs. Client extensions remain with their operation family.
- `operations.rs` and `operations/`: the existing thin native-operation layout.

Test roots follow the requested dialect/asset/stream/persistence/error/embedding/
operation/backend groups. Cross-dialect tests remain single tests in the shared
stream or diagnostics groups, preserving their assertions and feature gates.
Shared data and scripted backends live in support/. Larger groups use child files
so the test roots remain easy to scan. No tests are named after development rounds.

The additional helper files and `typescript.rs` are the only layout refinements
beyond the target: they separate existing responsibilities and keep files short.

## Verification

| Configuration | Before | After |
|---|---:|---:|
| Default features | 92 | 92 |
| s3,ts | 107 | 107 |

Counts include doctests. All 172 integration-test/helper function bodies are
unchanged token-for-token, including literal contents. No tests or assertions were
added or removed. All 81 exported TS files have the same paths and bytes as before,
and pass strict TypeScript checking. Rust checks cover full s3 wasm, minimal remote
wasm, strict Clippy for default/s3,ts and independent feature configurations, plus
rustfmt.

## Rust file tree

Line counts are physical lines after rustfmt. Documentation and Cargo metadata
are omitted from this Rust-only inventory.

```text
src/
├── assets/
│   ├── mod.rs (50)
│   ├── capabilities.rs (328)
│   ├── prepare.rs (308)
│   ├── resolve.rs (151)
│   ├── s3.rs (342)
│   └── validation.rs (248)
├── backend/
│   ├── mod.rs (35)
│   ├── embedded.rs (112)
│   └── remote.rs (68)
├── dialects/
│   ├── chat/
│   │   ├── render/
│   │   │   └── roles.rs (178)
│   │   ├── stream/
│   │   │   └── choice.rs (254)
│   │   ├── mod.rs (20)
│   │   ├── audio.rs (69)
│   │   ├── metadata.rs (101)
│   │   ├── parse.rs (302)
│   │   ├── render.rs (322)
│   │   ├── stream.rs (215)
│   │   └── warnings.rs (30)
│   ├── claude/
│   │   ├── parse/
│   │   │   └── content.rs (242)
│   │   ├── render/
│   │   │   └── blocks.rs (201)
│   │   ├── mod.rs (15)
│   │   ├── metadata.rs (82)
│   │   ├── parse.rs (226)
│   │   ├── render.rs (285)
│   │   ├── stream.rs (329)
│   │   └── warnings.rs (52)
│   ├── gemini/
│   │   ├── parse/
│   │   │   └── content.rs (209)
│   │   ├── render/
│   │   │   └── content.rs (166)
│   │   ├── mod.rs (20)
│   │   ├── metadata.rs (153)
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
│   │   ├── mod.rs (20)
│   │   ├── metadata.rs (273)
│   │   ├── parse.rs (293)
│   │   ├── render.rs (301)
│   │   ├── stream.rs (161)
│   │   ├── tools.rs (61)
│   │   └── warnings.rs (21)
│   ├── shared/
│   │   ├── citations/
│   │   │   ├── mod.rs (112)
│   │   │   ├── claude.rs (253)
│   │   │   ├── gemini.rs (112)
│   │   │   └── openai.rs (121)
│   │   ├── warnings/
│   │   │   ├── mod.rs (190)
│   │   │   ├── content.rs (180)
│   │   │   └── fields.rs (164)
│   │   ├── mod.rs (17)
│   │   ├── media.rs (282)
│   │   ├── reasoning.rs (243)
│   │   ├── sources.rs (222)
│   │   └── wire.rs (125)
│   ├── mod.rs (103)
│   └── parse.rs (24)
├── embeddings/
│   ├── mod.rs (172)
│   ├── client.rs (76)
│   ├── gemini.rs (240)
│   └── openai.rs (158)
├── ir/
│   ├── mod.rs (26)
│   ├── citation.rs (85)
│   ├── content.rs (102)
│   ├── media.rs (138)
│   ├── message.rs (39)
│   ├── model.rs (64)
│   ├── request.rs (102)
│   ├── result.rs (94)
│   ├── tool.rs (54)
│   └── warning.rs (31)
├── operations/
│   ├── audio.rs (132)
│   ├── count_tokens.rs (126)
│   ├── json.rs (83)
│   ├── models.rs (20)
│   └── video.rs (62)
├── options/
│   ├── mod.rs (58)
│   ├── claude.rs (48)
│   ├── gemini.rs (36)
│   └── openai.rs (204)
├── stream/
│   ├── mod.rs (192)
│   ├── decoder.rs (127)
│   └── sse.rs (126)
├── client.rs (258)
├── error.rs (247)
├── lib.rs (67)
├── operations.rs (78)
└── typescript.rs (43)

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
│   ├── mod.rs (196)
│   ├── content.rs (70)
│   ├── generation.rs (95)
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
├── operations.rs (275)
├── persistence.rs (194)
├── s3.rs (40)
└── stream.rs (24)
```

No Rust file exceeds 400 lines. The largest source file is `src/dialects/gemini/parts.rs` (387 lines); the largest overall is `tests/embeddings.rs` (392 lines).
