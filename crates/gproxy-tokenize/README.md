# gproxy-tokenize

[![crates.io](https://img.shields.io/crates/v/gproxy-tokenize.svg)](https://crates.io/crates/gproxy-tokenize)
[![docs.rs](https://docs.rs/gproxy-tokenize/badge.svg)](https://docs.rs/gproxy-tokenize)
[![license](https://img.shields.io/crates/l/gproxy-tokenize.svg)](LICENSE)

Count tokens of an LLM request body locally, without calling the provider.

`gproxy-tokenize` is the local token-counting layer of
[GPROXY](https://github.com/LeenHawk/gproxy). It takes a provider-native JSON
body (OpenAI chat/responses, Claude messages, Gemini generateContent), harvests
the text it contains, and counts it.

## Usage

```toml
[dependencies]
gproxy-tokenize = { version = "=2.3.0", features = ["count-local"] }
```

```rust
use gproxy_tokenize::{count, count_text};

// A single text buffer.
let n = count_text("Hello, how are you today?");

// A provider-native request body, counted against a model.
let body = br#"{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}"#;
let n = count("gpt-4o-mini", body, None, &registry);
```

`registry` is a `&TokenizerRegistry` (see below). Without the `count-local`
feature it is `()` instead, so call sites stay identical across native and edge
builds.

`count` never fails. Worst case it degrades to the character estimate, so a
counting path can always produce a number. Use `count_detailed` when the caller
needs the method, vocabulary, and warnings, or async `try_count` when malformed
JSON and an unavailable requested tokenizer must be errors.

## Counting ladder

1. **tiktoken** for the gpt families (`gpt-3.5`, `gpt-4`, `gpt-4o`, `gpt-4.1`,
   `gpt-5`, `o1`, `o3`, `o4`) — exact vocabularies compiled in.
2. **Hugging Face `tokenizers`** for everything else, resolved through a
   `TokenizerRegistry`. A `tokenizer_map` of `glob → vocab name` can redirect a
   model to a specific vocabulary; otherwise the model name itself is looked up.
3. **Bundled fallback vocabulary** when the model resolves to nothing.
4. **Character estimate** (`chars / 2`) as the floor.

Each message adds a fixed framing overhead, so the result approximates what a
provider bills rather than raw text tokens.

Overlapping `tokenizer_map` globs have deterministic priority: the pattern with
the most non-`*` bytes wins, with lexical pattern order breaking ties. Glob
matching is anchored and supports only `*`.

## Registry

`TokenizerRegistry` is the cache for Hugging Face vocabularies. It is wired to
the host application through two traits, so this crate itself performs no I/O:

- `TokenizerStore` — persistence (list / get / put a vocabulary by name).
- `TokenizerClient` — outbound HTTP, used to hydrate a missing vocabulary.

Lookups are non-blocking: a miss returns `None` and schedules a background load
via `request_load`, while the caller falls further down the ladder.

## Features

| Feature            | Default | Effect                                                   |
| ------------------ | ------- | -------------------------------------------------------- |
| `tiktoken`         | off     | Built-in OpenAI-family vocabularies.                     |
| `hf-registry`      | off     | Hugging Face tokenizer registry and host I/O traits.     |
| `bundled-fallback` | off     | 6.1 MiB DeepSeek fallback asset; implies `hf-registry`.  |
| `count-local`      | off     | Compatibility bundle enabling all three features above. |

With `count-local` off, the crate has a single dependency (`serde_json`) and
compiles to `wasm32`; only `harvest`, `is_gpt_family`, and the character
estimate remain. This is the edge/serverless build.

## License

Licensed under the [MIT License](LICENSE).

The bundled fallback vocabulary under `assets/tokenizers/` is a third-party
tokenizer vocabulary. Its source revision, checksum, and license are recorded
in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
