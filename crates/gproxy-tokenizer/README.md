# gproxy-tokenizer

[中文](README.zh-CN.md) · English

A synchronous local tokenizer: select by model, then **string in, token count out**.

```rust
# #[cfg(feature = "local")]
# {
use gproxy_tokenizer::Tokenizer;

let tokenizer = Tokenizer::for_model("gpt-5", None)?;
assert_eq!(tokenizer.count("hello world")?, 2);

// Non-GPT model without a supplied vocabulary: shared DeepSeek V4 encoder.
let tokenizer = Tokenizer::for_model("claude-sonnet-4", None)?;
let tokens = tokenizer.count("Hello, 世界!")?;
# }
# Ok::<(), gproxy_tokenizer::CountError>(())
```

Selection is fixed:

1. GPT family → tiktoken, even when a vocabulary is supplied.
2. Other models with a supplied vocabulary → that vocabulary.
3. Other models without one → bundled DeepSeek V4 Pro.

GPT names use tiktoken's model mapping, including GPT-3.5/4, GPT-4o/4.1/5 and
GPT-OSS. ChatGPT, o1/o3/o4 and Codex Mini names are also recognized. New GPT names
unknown to tiktoken use `o200k_base`. Pass the actual model name, without a provider
prefix. A fallback vocabulary does not imply equivalence to the model's native tokenizer.

## Supplied vocabularies

Load `tokenizer.json` bytes once with `Vocabulary::from_bytes(&bytes)?`, then pass
`Some(&vocabulary)` to `Tokenizer::for_model(model, ...)`. Core can retain the
vocabulary and reuse it across requests and models.

```rust
# #[cfg(feature = "local")]
# {
use gproxy_tokenizer::{CountError, Tokenizer, Vocabulary};

fn count_with_vocab(vocab: &Vocabulary, text: &str) -> Result<u64, CountError> {
    Tokenizer::for_model("my-model", Some(vocab))?.count(text)
}
# }
```

`Vocabulary` and `Tokenizer` share encoders through `Arc`: selection and cloning
only copy a shared handle, never the vocabulary contents. Parsing borrows the JSON
bytes and allocates the encoder once. Bundled DeepSeek bytes are embedded once;
the encoder is initialized once on first use and shared thereafter. Tiktoken also
uses shared singleton encoders.

`count(&str) -> Result<u64, CountError>` encodes the supplied string directly.
It does not parse requests, join text, apply chat templates, or add message
overhead/special tokens. HF padding and truncation are disabled. Invalid
vocabularies and encoding failures return errors; a failed supplied vocabulary
does not silently switch to DeepSeek or character estimation.

## Features and ownership

Defaults enable `local`, including `tiktoken`, `huggingface` and
`bundled-deepseek`. `for_model` is available with `local`. These features can also
be enabled individually for explicit encoder selection through `cl100k_base`,
`o200k_base`, `from_vocabulary` or `deepseek_v4_pro`. With default features disabled,
`character_estimate()` provides dependency-free `ceil(Unicode scalars / 2)`;
character estimation is never part of the automatic selection chain.

Native/WASM builds use Rust regex; wasm32 enables HF's WASM support. Core/callers
own request preparation, custom model-to-vocabulary configuration, downloads,
authentication, persistence and custom vocabulary caching. Management belongs to
manage. This crate has no network, database, Tokio, registry or background tasks.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
[LICENSE](assets/tokenizers/LICENSE) for DeepSeek attribution and licensing.
