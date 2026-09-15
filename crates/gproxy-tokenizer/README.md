# gproxy-tokenizer

Small, synchronous, local token counting for GPROXY v4. No HTTP client, database,
Tokio, registry, model-name matching, global configuration or background tasks.

```rust
use gproxy_tokenizer::{RequestFormat, Tokenizer};

let tokenizer = Tokenizer::character_estimate();
assert_eq!(tokenizer.count_text("你好 world")?, 4);
let body = br#"{"messages":[{"role":"user","content":"hello world"}]}"#;
assert_eq!(tokenizer.count_request(RequestFormat::OpenAiChat, body)?, 6);
# Ok::<(), gproxy_tokenizer::CountError>(())
```

| Feature | Constructors |
|---|---|
| default (none) | `Tokenizer::character_estimate()`; only depends on serde_json |
| `tiktoken` | `Tokenizer::cl100k_base()`, `Tokenizer::o200k_base()` |
| `huggingface` | `Tokenizer::from_bytes(tokenizer_json)` |
| `bundled-deepseek` | `Tokenizer::deepseek_v4_pro()`; includes `huggingface` |
| `local` | All local backends above |

The tokenizer is selected explicitly and reused for both `count_text` and
`count_request`. Hugging Face instances disable padding and truncation, and
encode without added special tokens. `from_bytes` accepts tokenizer.json bytes,
not a repository name, file path or tokenizer_config.json/chat template.
The bundled constructor parses the included asset; retain its instance rather
than constructing it per request. Native and wasm builds use Rust regex; HF's
WASM support is enabled on wasm32. Heavy vocabularies remain opt-in.

## Request counting

`RequestFormat` selects Chat, Responses, Claude or Gemini JSON. The caller
validates the protocol schema and supplies the final target request. Generation
and CountTokens request bodies use their respective format (OpenAI CountTokens
uses Responses). Gemini embedded `generateContentRequest` is supported.

Collect visible text and serialized tool/schema configuration, join fragments
with newlines, and encode. No fixed overhead is added per message or input item.
Top-level projection follows the selected format instead of recursively scanning
unrelated metadata. This is an estimate, not a model-specific chat template.

Only supplied content is counted. Media token costs, opaque signatures, file IDs,
cached-content references and previous-response IDs are not counted as text or
resolved. A text document's supplied source is included. Upstream usage and
CountTokens responses are outside this crate's responsibilities.

Malformed JSON, unusable vocabularies and encoding failures return errors; there
is no silent tokenizer switch. The caller can explicitly use the character
encoder, which computes `ceil(Unicode scalar values / 2)`.

## Core boundary

Core owns model-to-vocabulary mapping, fallback selection, instance caching,
download authentication, persistence, load deduplication and preheating. It
obtains bytes via its client/store capabilities and passes them to `from_bytes`.
Management operations belong to manage. These integrations will be implemented
with v4 core/manage; this crate does not restore the v3 registry or its I/O traits.

The DeepSeek asset and its original attribution are retained; see
`THIRD_PARTY_NOTICES.md` and `assets/tokenizers/LICENSE`.
