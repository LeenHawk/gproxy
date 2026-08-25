# gproxy-tokenize

Offline token counting for provider-native OpenAI, Claude, and Gemini request
bodies.

The counting ladder is deterministic:

1. built-in tiktoken vocabularies for GPT, o1, o3, and o4 model families;
2. a persisted or downloaded Hugging Face tokenizer selected by the most
   specific `tokenizer_map` glob, or by the model name;
3. the bundled DeepSeek V4 Pro tokenizer;
4. `ceil(Unicode scalar values / 2)` when no usable tokenizer exists. This is
   always the final rung on edge builds, where local tokenizer features stay
   disabled.

Provider message arrays add four tokens per item for wire framing. `count` is
total: malformed JSON and tokenizer failures fall down the ladder. `try_count`
is the strict alternative.

The default feature set only depends on `serde_json` and compiles for
`wasm32-unknown-unknown`. Native hosts enable `count-local`; registry I/O is
provided by the host through `TokenizerStore` and `TokenizerClient`.
