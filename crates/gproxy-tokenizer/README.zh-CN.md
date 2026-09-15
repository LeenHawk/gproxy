# gproxy-tokenizer

中文 · [English](README.md)

同步本地分词计数库：按模型选择分词器，然后**输入字符串，输出 token 数**。

```rust
use gproxy_tokenizer::Tokenizer;

let tokenizer = Tokenizer::for_model("gpt-5", None)?;
assert_eq!(tokenizer.count("hello world")?, 2);

// 非 GPT 模型，未提供词表：使用共享的内置 DeepSeek V4。
let tokenizer = Tokenizer::for_model("claude-sonnet-4", None)?;
let tokens = tokenizer.count("Hello, 世界!")?;
```

选择顺序固定：

1. GPT 系列 → tiktoken，即使传入了词表也优先使用 tiktoken。
2. 其他模型，提供了词表 → 使用该词表。
3. 其他模型，没有词表 → 内置 DeepSeek V4 Pro。

GPT 名称按 tiktoken 的模型映射选择词表，包括 GPT-3.5/4、GPT-4o/4.1/5 和 GPT-OSS，
也识别 ChatGPT、o1/o3/o4、Codex Mini。tiktoken 尚未识别的新 GPT 名称使用
`o200k_base`。调用方传入不带 provider 前缀的实际模型名。
回退词表不代表与该模型原生 tokenizer 的结果相同。

## 提供词表

通过 `Vocabulary::from_bytes(&bytes)?` 解析一次 `tokenizer.json`，再把
`Some(&vocabulary)` 传给 `Tokenizer::for_model(model, ...)`。
core 可持有词表，跨请求、跨模型复用。

```rust
use gproxy_tokenizer::{CountError, Tokenizer, Vocabulary};

fn count_with_vocab(vocab: &Vocabulary, text: &str) -> Result<u64, CountError> {
    Tokenizer::for_model("my-model", Some(vocab))?.count(text)
}
```

`Vocabulary` 和 `Tokenizer` 通过 `Arc` 共享编码器：选择或克隆时只复制共享句柄，
不复制词表内容。解析时借用 JSON 字节，一次性构建编码器。内置 DeepSeek 字节只
嵌入一份，编码器首次使用时只解析一次，后续共享；tiktoken 同样复用单例编码器。

`count(&str) -> Result<u64, CountError>` 直接编码输入字符串，不解析请求、不拼接
文本、不应用 chat template，不添加消息固定开销或 special tokens。HF 词表关闭
padding/truncation。无效词表与编码失败返回错误；传入词表出错时不会悄悄切换到
DeepSeek 或字符估算。

## Feature 与职责

默认启用 `local`，包含 `tiktoken`、`huggingface`、`bundled-deepseek`。
`for_model` 在 `local` 下可用。也可分别启用这些 feature，通过 `cl100k_base`、
`o200k_base`、`from_vocabulary` 或 `deepseek_v4_pro` 显式选择编码器。
关闭默认 feature 时，`character_estimate()` 提供无第三方依赖的
`ceil(Unicode 标量数 / 2)`；字符估算不参与自动选择链。

native/WASM 使用 Rust regex，wasm32 启用 HF 的 WASM 支持。请求内容准备、
自定义模型到词表的配置、下载、认证、持久化和自定义词表缓存属于 core/调用方，
管理操作属于 manage。库内没有网络、数据库、Tokio、registry 或后台任务。

DeepSeek 词表归属和许可证见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
及 [LICENSE](assets/tokenizers/LICENSE)。
