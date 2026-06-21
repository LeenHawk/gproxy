# Agent CLI TLS 指纹参考

v1 只有全局 `GPROXY_SPOOF` / `spoof_emulation` 这类浏览器 TLS preset。v2 的模型更细：

- agent channel 可以有内置的 TLS/HTTP2 emulation；
- provider 可以配置默认 `tls_fingerprint`；
- credential 可以覆盖 provider 的 `tls_fingerprint`；
- upstream client pool 按 `(proxy, fingerprint)` 缓存 native `wreq` client。

这页记录 agent CLI 模型路径的目标指纹、v2 当前能复刻的边界，以及
`tls_fingerprint` JSON 的维护格式。

实现位置：

- `src/channel/bulletins/*/fingerprint.rs`：channel 内置 profile。
- `src/channel/mod.rs`：`Channel::default_emulation`。
- `src/channel/resolve.rs`：credential 覆盖 provider 的解析规则。
- `src/http/client/fingerprint.rs`：JSON blob 到 `wreq::Emulation` 的映射。
- `src/http/client/pool.rs`：按 `(proxy, fingerprint)` 复用 upstream client。
- `src/api/tls_presets.rs`：Console preset 列表。

## 1. 生效边界

TLS emulation 只适用于 native + `upstream-wreq` 构建。edge wasm 使用平台 `fetch`，
不能控制 ClientHello、HTTP2 SETTINGS 或本地代理栈。

生效优先级：

1. credential `tls_fingerprint`
2. provider `tls_fingerprint`
3. channel `default_emulation()`
4. 默认 `wreq` client

只要 provider 或 credential 明确配置了 `tls_fingerprint`，但该 JSON 不能映射为可用
emulation，请求会失败，不会静默降级到默认 TLS。这样可以避免“以为在伪装，实际没伪装”的
运行状态。

## 2. 模型路径目标

下表记录的是模型调用路径，不是登录、usage、遥测或内部检查路径。

| Channel | 真实客户端栈 | 模型路径协议 | 目标 JA4 | 内置 profile |
| --- | --- | --- | --- | --- |
| `claudecode` | Node/OpenSSL | HTTP/1.1 | `t13d1714h1_5b57614c22b0_43ade6aba3df` | `src/channel/bulletins/claudecode/fingerprint.rs` |
| `codex` | rustls/hyper | HTTP/2 | `t13d1011h2_61a7ad8aa9b6_3fcd1a44f3e3` | `src/channel/bulletins/codex/fingerprint.rs` |
| `geminicli` | system OpenSSL | HTTP/1.1 | `t13d521100_b262b3658495_8e6e362c5eac` | best-effort subset |
| `antigravity` | Go `crypto/tls` | HTTP/1.1, no ALPN | `t13d131100_f57a46bbacb6_ab7e3b40a677` | best-effort, TLS1.3 order preserved |
| `copilotcli` | rustls | HTTP/1.1 | `t13d1011h1_61a7ad8aa9b6_*` | model-path rustls profile |
| `kiro` | rustls/aws-lc | HTTP/1.1, no ALPN | `t13d101000_61a7ad8aa9b6_3fcd1a44f3e3` | rustls-style subset |

UA 与 TLS profile 要一起维护。请求头目标在 `docs/agent-request-headers.md`，本页只记录
transport 指纹。

## 3. 保真边界

v2 native transport 使用 `wreq`，底层可配置项不是任意 TLS 字节流。可以稳定控制：

- UA/default headers；
- ALPN；
- GREASE 开关；
- TLS 版本范围；
- BoringSSL 支持的 cipher / curve / sigalg token list；
- 部分 extension 顺序；
- HTTP2 SETTINGS、SETTINGS 顺序、connection window、pseudo-header 顺序。

不能保证逐字节复刻：

- system OpenSSL 的 52 cipher 全量列表；
- ARIA、CAMELLIA、CCM、DHE、静态 RSA-CBC 等 BoringSSL 不发的套件；
- `x448`、`ffdhe`、部分实验 sigalg；
- Go 和 rustls/aws-lc 的扩展集细节；
- 自定义 PQC group；
- JA3 中受扩展乱序影响的 hash。

因此文档和代码都优先使用 JA4 判断大类。`geminicli` 这类 OpenSSL 全量栈只能做到
“UA 精确 + ALPN/HTTP1 精确 + AEAD 子集近似”，不能承诺 JA4 完全相同。

## 4. HTTP/2 指纹

模型路径里当前只有 `codex` 走 HTTP/2。它的 Akamai h2 指纹目标是：

```text
2:0;4:2097152;5:16384;6:16384|5177345|0|m,s,a,p
```

对应 v2 设置：

| 字段 | 值 |
| --- | --- |
| `enable_push` | `false` |
| `initial_window_size` | `2097152` |
| `initial_connection_window_size` | `5242880` |
| `max_frame_size` | `16384` |
| `max_header_list_size` | `16384` |
| `headers_pseudo_order` | `[":method", ":scheme", ":authority", ":path"]` |
| `settings_order` | `[2, 4, 5, 6]` |

`claudecode`、`geminicli`、`antigravity`、`copilotcli`、`kiro` 的模型路径都按
HTTP/1.1 处理。Antigravity 的遥测/OAuth 可能走 h2，但不是模型路径，不应该污染模型
channel profile。

## 5. `tls_fingerprint` JSON 草案

存储字段：

- provider: `providers.tls_fingerprint`
- credential: `credentials.tls_fingerprint`

JSON 顶层只识别三个运行字段：`headers`、`tls`、`http2`。`_reference`、`_fidelity`、
`_unsupported` 这类下划线字段可作为注释保存；解析器会忽略它们。

```json
{
  "headers": {
    "user-agent": "codex_exec/0.137.0 (Debian 13.0.0; x86_64) xterm-256color"
  },
  "tls": {
    "alpn_protocols": ["h2"],
    "grease_enabled": false,
    "min_tls_version": "tls1.2",
    "max_tls_version": "tls1.3",
    "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384",
    "curves_list": "X25519:P-256:P-384"
  },
  "http2": {
    "enable_push": false,
    "initial_window_size": 2097152,
    "initial_connection_window_size": 5242880,
    "max_frame_size": 16384,
    "max_header_list_size": 16384,
    "headers_pseudo_order": [":method", ":scheme", ":authority", ":path"],
    "settings_order": [2, 4, 5, 6]
  },
  "_reference": {
    "channel": "codex",
    "ja4": "t13d1011h2_61a7ad8aa9b6_3fcd1a44f3e3"
  },
  "_fidelity": "medium"
}
```

字段说明：

| JSON key | 作用 |
| --- | --- |
| `headers` | 默认请求头。通常只放 UA；channel 代码仍会在每个请求上注入 auth/protocol headers。 |
| `tls.alpn_protocols` | `["h2"]`、`["http/1.1"]` 或 `[]`。空数组表示不发送 ALPN。 |
| `tls.grease_enabled` | 这些 CLI 样本通常为 `false`。 |
| `tls.min_tls_version` / `tls.max_tls_version` | `tls1.0` 到 `tls1.3`。 |
| `tls.cipher_list` | BoringSSL token，冒号分隔。 |
| `tls.curves_list` | curve/group 列表。 |
| `tls.sigalgs_list` | 签名算法列表。 |
| `tls.preserve_tls13_cipher_list` | 保持 TLS1.3 cipher 顺序，主要用于 Go 样式近似。 |
| `tls.extension_permutation` | u16 extension id 数组，只能排列 BoringSSL 实际会发的 extension。 |
| `http2` object | 启用并设置 HTTP/2 指纹。 |
| `http2: false` | 对解析器是 no-op；HTTP/1.1 由 `tls.alpn_protocols` 决定。 |

Console 通过 `GET /admin/tls-presets` 读取静态 preset。preset 是可存储的精简 blob，
不是完整抓包记录；channel 内置 profile 的 Rust 源仍是运行时默认行为的准绳。

## 6. 内置 profile 维护

每个 agent channel 的 `fingerprint.rs` 应只描述 transport profile，不负责 auth header
或协议 header。UA 是否放进 emulation 要看使用方式：

- channel 内置 `default_emulation()` 通常不放 UA，因为 auth 模块会逐请求注入 UA；
- Console preset 可以带 `headers.user-agent`，因为它是一个独立可存储 blob；
- 如果 provider/credential 配置了 `tls_fingerprint`，它会覆盖 channel 内置 profile。

新增或更新 profile 时：

1. 重抓真实模型路径的 TLS、UA、HTTP2。
2. 确认它不是登录、usage、遥测或内部检查路径。
3. 更新 `docs/agent-request-headers.md` 的 UA/header 目标。
4. 更新对应 `src/channel/bulletins/<channel>/fingerprint.rs`。
5. 如果要暴露给 Console，更新 `src/api/tls_presets.rs`。
6. 跑 native `upstream-wreq` 相关测试，至少覆盖 profile 可以构建。

可用的重点测试：

```bash
cargo test --features full channel_default_emulations_build
cargo test --features full tls_presets_valid
cargo test --features full fingerprint
```

## 7. 采集注意事项

采集命令应固定真实模型路径和一次性 prompt。常用脚本：

```bash
python3 scripts/capture_tls_fp.py 8888 proxy
python3 scripts/capture_h2_fp.py 8888 /tmp/fpca.pem
python3 scripts/capture_fwd_mitm.py 8889 /tmp/fpca.pem
```

经验规则：

- 先分清模型路径和非模型路径；Codex、Copilot、Kiro 都有不止一套网络画像。
- 优先记录 JA4，JA3 容易受 extension order 影响。
- 记录 ALPN 和 HTTP2 SETTINGS；只看 TLS 不够。
- 脱敏 token、cookie、prompt 和账户信息。
- 升级真实 CLI 后不要只更新 UA；同步复核 header、TLS、HTTP2 和 body shaping。

## English

# Agent CLI TLS Fingerprint Reference

v1 had global browser-style TLS presets such as `GPROXY_SPOOF` /
`spoof_emulation`. v2 is more granular:

- agent channels can provide built-in TLS/HTTP2 emulation;
- providers can configure a default `tls_fingerprint`;
- credentials can override the provider `tls_fingerprint`;
- the upstream client pool caches native `wreq` clients by `(proxy, fingerprint)`.

This page records the model-path targets for agent CLI fingerprints, the fidelity
limits of current v2, and the maintained `tls_fingerprint` JSON format.

Implementation locations:

- `src/channel/bulletins/*/fingerprint.rs`: built-in channel profiles.
- `src/channel/mod.rs`: `Channel::default_emulation`.
- `src/channel/resolve.rs`: credential-over-provider resolution.
- `src/http/client/fingerprint.rs`: JSON blob to `wreq::Emulation` mapping.
- `src/http/client/pool.rs`: upstream client reuse keyed by `(proxy, fingerprint)`.
- `src/api/tls_presets.rs`: Console preset list.

## 1. Scope

TLS emulation applies only to native builds with `upstream-wreq`. Edge wasm uses
platform `fetch` and cannot control ClientHello, HTTP2 SETTINGS, or local proxy
transport behavior.

Priority:

1. credential `tls_fingerprint`
2. provider `tls_fingerprint`
3. channel `default_emulation()`
4. default `wreq` client

If a provider or credential explicitly configures `tls_fingerprint` but the JSON
does not map to usable emulation, the request fails. It does not silently fall
back to default TLS, which prevents a hidden "configured but not actually
emulating" state.

## 2. Model-Path Targets

These are model-call paths, not login, usage, telemetry, or internal check paths.

| Channel | Real client stack | Model-path protocol | Target JA4 | Built-in profile |
| --- | --- | --- | --- | --- |
| `claudecode` | Node/OpenSSL | HTTP/1.1 | `t13d1714h1_5b57614c22b0_43ade6aba3df` | `src/channel/bulletins/claudecode/fingerprint.rs` |
| `codex` | rustls/hyper | HTTP/2 | `t13d1011h2_61a7ad8aa9b6_3fcd1a44f3e3` | `src/channel/bulletins/codex/fingerprint.rs` |
| `geminicli` | system OpenSSL | HTTP/1.1 | `t13d521100_b262b3658495_8e6e362c5eac` | best-effort subset |
| `antigravity` | Go `crypto/tls` | HTTP/1.1, no ALPN | `t13d131100_f57a46bbacb6_ab7e3b40a677` | best-effort, TLS1.3 order preserved |
| `copilotcli` | rustls | HTTP/1.1 | `t13d1011h1_61a7ad8aa9b6_*` | model-path rustls profile |
| `kiro` | rustls/aws-lc | HTTP/1.1, no ALPN | `t13d101000_61a7ad8aa9b6_3fcd1a44f3e3` | rustls-style subset |

Maintain UA and TLS profile together. Header targets live in
`docs/agent-request-headers.md`; this page covers transport fingerprints.

## 3. Fidelity Limits

v2 native transport uses `wreq`. Its configurable surface is not an arbitrary
TLS byte stream. It can reliably control:

- UA/default headers;
- ALPN;
- GREASE flag;
- TLS version range;
- BoringSSL-supported cipher / curve / sigalg token lists;
- some extension ordering;
- HTTP2 SETTINGS, SETTINGS order, connection window, and pseudo-header order.

It cannot guarantee byte-exact reproduction of:

- full 52-cipher system OpenSSL lists;
- ARIA, CAMELLIA, CCM, DHE, static RSA-CBC, and other suites BoringSSL does not
  send;
- `x448`, `ffdhe`, and some experimental sigalgs;
- Go and rustls/aws-lc extension-set details;
- custom PQC groups;
- JA3 hashes affected by extension ordering.

Docs and code therefore prefer JA4 for class-level matching. For clients such as
`geminicli` that use full OpenSSL, v2 can do "exact UA + exact ALPN/HTTP1 +
approximate AEAD subset", but not guaranteed exact JA4.

## 4. HTTP/2 Fingerprint

Only `codex` currently uses HTTP/2 on the model path. Its Akamai h2 target is:

```text
2:0;4:2097152;5:16384;6:16384|5177345|0|m,s,a,p
```

Mapped v2 settings:

| Field | Value |
| --- | --- |
| `enable_push` | `false` |
| `initial_window_size` | `2097152` |
| `initial_connection_window_size` | `5242880` |
| `max_frame_size` | `16384` |
| `max_header_list_size` | `16384` |
| `headers_pseudo_order` | `[":method", ":scheme", ":authority", ":path"]` |
| `settings_order` | `[2, 4, 5, 6]` |

`claudecode`, `geminicli`, `antigravity`, `copilotcli`, and `kiro` model paths
are treated as HTTP/1.1. Antigravity telemetry/OAuth may use h2, but that is not
the model path and should not pollute the model channel profile.

## 5. `tls_fingerprint` JSON Draft

Storage fields:

- provider: `providers.tls_fingerprint`
- credential: `credentials.tls_fingerprint`

Only three runtime top-level keys are recognized: `headers`, `tls`, and `http2`.
Underscore-prefixed keys such as `_reference`, `_fidelity`, and `_unsupported`
can be stored as comments; the parser ignores them.

```json
{
  "headers": {
    "user-agent": "codex_exec/0.137.0 (Debian 13.0.0; x86_64) xterm-256color"
  },
  "tls": {
    "alpn_protocols": ["h2"],
    "grease_enabled": false,
    "min_tls_version": "tls1.2",
    "max_tls_version": "tls1.3",
    "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384",
    "curves_list": "X25519:P-256:P-384"
  },
  "http2": {
    "enable_push": false,
    "initial_window_size": 2097152,
    "initial_connection_window_size": 5242880,
    "max_frame_size": 16384,
    "max_header_list_size": 16384,
    "headers_pseudo_order": [":method", ":scheme", ":authority", ":path"],
    "settings_order": [2, 4, 5, 6]
  },
  "_reference": {
    "channel": "codex",
    "ja4": "t13d1011h2_61a7ad8aa9b6_3fcd1a44f3e3"
  },
  "_fidelity": "medium"
}
```

Field meanings:

| JSON key | Meaning |
| --- | --- |
| `headers` | Default request headers. Usually only UA; channel code still injects auth/protocol headers per request. |
| `tls.alpn_protocols` | `["h2"]`, `["http/1.1"]`, or `[]`. Empty array means send no ALPN. |
| `tls.grease_enabled` | Usually `false` for these CLI samples. |
| `tls.min_tls_version` / `tls.max_tls_version` | `tls1.0` through `tls1.3`. |
| `tls.cipher_list` | BoringSSL tokens separated by colons. |
| `tls.curves_list` | Curve/group list. |
| `tls.sigalgs_list` | Signature algorithm list. |
| `tls.preserve_tls13_cipher_list` | Preserve TLS1.3 cipher order, mainly for Go-style approximation. |
| `tls.extension_permutation` | Array of u16 extension ids; can only reorder extensions BoringSSL actually sends. |
| `http2` object | Enables and configures the HTTP/2 fingerprint. |
| `http2: false` | No-op for the parser; HTTP/1.1 is controlled by `tls.alpn_protocols`. |

Console reads static presets from `GET /admin/tls-presets`. A preset is a
minimal storable blob, not a full capture log. The Rust channel built-in profile
is still the runtime default source of truth.

## 6. Maintaining Built-In Profiles

Each agent channel's `fingerprint.rs` should describe transport profile only. It
should not own auth headers or protocol headers. Whether UA belongs in emulation
depends on usage:

- channel built-in `default_emulation()` usually does not include UA because the
  auth module injects UA per request;
- Console presets can include `headers.user-agent` because they are standalone
  stored blobs;
- a provider/credential `tls_fingerprint` overrides the channel built-in
  profile.

When adding or updating a profile:

1. Re-capture the real model path TLS, UA, and HTTP2.
2. Confirm it is not login, usage, telemetry, or an internal check path.
3. Update `docs/agent-request-headers.md` for UA/header targets.
4. Update `src/channel/bulletins/<channel>/fingerprint.rs`.
5. If it should be exposed in Console, update `src/api/tls_presets.rs`.
6. Run native `upstream-wreq` tests, at least covering that the profile builds.

Useful tests:

```bash
cargo test --features full channel_default_emulations_build
cargo test --features full tls_presets_valid
cargo test --features full fingerprint
```

## 7. Capture Notes

Capture commands should pin the real model path and a one-shot prompt. Common
scripts:

```bash
python3 scripts/capture_tls_fp.py 8888 proxy
python3 scripts/capture_h2_fp.py 8888 /tmp/fpca.pem
python3 scripts/capture_fwd_mitm.py 8889 /tmp/fpca.pem
```

Rules of thumb:

- Separate model paths from non-model paths; Codex, Copilot, and Kiro each have
  more than one network profile.
- Prefer JA4; JA3 is more sensitive to extension order.
- Record ALPN and HTTP2 SETTINGS; TLS alone is insufficient.
- Redact tokens, cookies, prompts, and account information.
- When upgrading a real CLI, update more than UA. Re-check headers, TLS, HTTP2,
  and body shaping together.
