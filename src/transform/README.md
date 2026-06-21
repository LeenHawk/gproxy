# Transform Layer

`src/transform` owns provider-to-provider conversion. `src/protocol` owns
provider wire models only.

## Rules

- Organize transforms by `Operation` / `OperationGroup`, not provider family.
- When an `OperationGroup` has distinct methods such as list/get, split those
  methods into subdirectories before provider-pair files.
- Keep transforms pairwise. Do not introduce a unified IR.
- Same-kind passthrough bypasses this layer.
- Pair files own real provider field differences.
- `common/` may only contain mechanical helpers:
  - SSE framing
  - role classification
  - tool id/result helpers
  - usage arithmetic
  - error construction helpers
  - metadata helpers
- Provider `extra` fields are not preserved by transforms. Pair files should
  drop source `extra` fields and initialize target `extra` fields as empty.

## Pair Files

Pair modules expose typed functions as they are implemented:

```rust
pub fn request(input: SourceRequest, ctx: &TransformContext) -> Result<TargetRequest, TransformError>;
pub fn response(input: SourceResponse, ctx: &TransformContext) -> Result<TargetResponse, TransformError>;
pub fn stream_event(input: SourceStreamEvent, ctx: &TransformContext) -> Result<TargetStreamEvent, TransformError>;
```

Only define functions that exist for that operation pair.

`StreamGenerateContent` resolves through the same content-generation pair matrix
as non-stream generation. Current `stream_event` functions are single-event,
stateless conversions; any cross-event aggregation for block indexes, tool call
identity, or final usage belongs in the runtime stream adapter.

If a pair grows past roughly 400-500 lines, split only that pair into:

```text
pair_name/
  mod.rs
  request.rs
  response.rs
  stream.rs
  tools.rs
```

## Runtime wiring (M2)

- `dispatch/{mod,content,other}.rs` — bytes-level
  `(TransformPair, ctx, body) -> body` covering all 37 wired pairs
  (content generation plus count_tokens/models/embeddings/images/compact);
  `is_wired` gates anything unported. M2.5: models pairs dispatch list/get on
  the source operation; request direction for body-less ops is path-synthesis
  only.
- `routing.rs` — compiled §8-B2 `routing_rules` + the
  passthrough/transform_to/local/unsupported decision.
- `stream_adapter.rs` — the runtime SSE adapter (decode upstream frames →
  `stream_event` per frame → encode inbound frames). Cross-event aggregation
  state, when needed, belongs here.
- local operations (models list/get, count_tokens) short-circuit in the failover loop — see pipeline/local_ops.rs and src/tokenize/.
