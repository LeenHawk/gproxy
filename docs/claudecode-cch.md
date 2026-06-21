# Claude Code CCH

`claudecode` channel 需要复刻 Claude Code 在 `POST /v1/messages` 请求体里写入的
`x-anthropic-billing-header`。这个 header 不是普通 HTTP header，而是插入到
Claude Messages body 的 `system[0].text` 中，并带一个 5 位 `cch` 校验值。

v2 的实现位置：

- `src/channel/bulletins/claudecode/mod.rs`：决定哪些请求需要注入 CCH。
- `src/channel/bulletins/claudecode/cch.rs`：生成 `metadata.user_id`、billing
  block、session id 和 checksum。
- `src/channel/bulletins/claudecode/auth.rs`：注入 OAuth bearer、`x-claude-code-session-id`
  和 Stainless/Claude CLI 请求头。

## 生效范围

CCH 只对最终上游路径精确等于 `POST /v1/messages` 的模型调用生效。

不要用前缀匹配。`POST /v1/messages/count_tokens` 虽然也在 Claude Messages API
下面，但 Anthropic 会拒绝额外的 `metadata` 字段，所以 v2 明确跳过该路径。

请求进入 CCH 前已经完成：

1. inbound operation classify；
2. OpenAI/Gemini 到 Claude Messages 的协议转换；
3. Claude body 整形，例如 cache-control hygiene 和 sampling 参数清理；
4. 可选的 magic cache 触发改写。

因此 CCH 必须覆盖 gproxy 自己最终要发送的 body bytes，而不是客户端原始 body。

## metadata.user_id

Claude Code 的 `metadata.user_id` 是一个 JSON 字符串，不是对象：

```json
{
  "metadata": {
    "user_id": "{\"device_id\":\"<device>\",\"account_uuid\":\"<account>\",\"session_id\":\"<session>\"}"
  }
}
```

字段来源：

| 字段 | v2 来源 |
| --- | --- |
| `device_id` | credential secret 中的稳定设备 id；缺失时按凭证派生。 |
| `account_uuid` | Claude Code OAuth/cookie 登录得到的账号 id；缺失时为空串。 |
| `session_id` | `cch::session_id(...)` 生成，同时作为 `x-claude-code-session-id` HTTP header。 |

`session_id` 不是每次完全随机。v2 用 `(device_id, system, first message, hour)` 选择
最多 1000 个稳定槽位，再生成 v4 形状 UUID。这样同一凭证不会无限增长会话 id 数量，
同时相近会话能保持稳定。

## Billing Block

`cch::apply` 会在 `system` 前面写入 billing text block：

```text
x-anthropic-billing-header: cc_version=2.1.162.553; cc_entrypoint=cli; cch=00000;
```

如果请求已经带有旧的 billing block，v2 会原地替换，而不是追加第二个 block。这样
重代理 Claude Code 请求时不会重复插入。

如果原始 `system` 是字符串，v2 会把它转换成 Claude block 数组：

```json
[
  { "type": "text", "text": "x-anthropic-billing-header: ..." },
  { "type": "text", "text": "<original system text>" }
]
```

## Checksum

算法来自 Claude Code 2.1.162 wire body 验证：

```text
cch = xxh64(final_body_bytes_with_cch_00000, seed=0x4d659218e32a3268) & 0xfffff
```

结果格式化为 5 位小写 hex，然后覆盖 `cch=00000;` 中的五个 `0`。

关键点：

- 输入是最终 JSON bytes，不是某个字段。
- `model`、`system`、`messages`、`tools`、`tool_choice` 都会影响结果。
- HTTP URL、HTTP headers、OAuth token 不参与计算。
- JSON 序列化形态会影响结果，所以实现对 `serde_json::to_vec` 的实际输出签名。
- 真实值写回后，服务端可以把它归零再重算并匹配。

## Header 关系

CCH body 里的 `session_id` 必须和 HTTP header
`x-claude-code-session-id` 使用同一个值。

当上游 base URL 是默认 `https://api.anthropic.com` 且请求是 `POST /v1/messages` 时，
v2 还会生成 `x-client-request-id`。自定义 base URL 不注入这个 header，保持和
真实 SDK 行为一致。

## 验证

相关单测在 `src/channel/bulletins/claudecode/cch.rs` 和
`src/channel/bulletins/claudecode/mod.rs`：

- `cch_matches_known_vector` 固定已知 body 的 `b3b78` 向量。
- `apply_injects_metadata_and_valid_cch` 验证 metadata、billing block 和回算结果。
- `apply_replaces_existing_billing_block` 验证重代理时不会重复插入。
- `count_tokens_skips_cch_metadata_injection` 防止 `count_tokens` 被误判为模型路径。

修改 Claude Code channel 时，至少跑：

```bash
cargo test --features full claudecode
```

如果升级 Claude Code 版本，需要重新确认 `cc_version`、entrypoint、seed、SDK header
集合和 wire body 行为。不要只相信 SDK raw-body 日志；最终 wire body 才是判断依据。

## English

# Claude Code CCH

The `claudecode` channel must reproduce the `x-anthropic-billing-header` that
Claude Code writes into the `POST /v1/messages` request body. This is not a
normal HTTP header. It is inserted into `system[0].text` in the Claude Messages
body and includes a 5-digit `cch` checksum.

v2 implementation locations:

- `src/channel/bulletins/claudecode/mod.rs`: decides which requests need CCH.
- `src/channel/bulletins/claudecode/cch.rs`: generates `metadata.user_id`, the
  billing block, session id, and checksum.
- `src/channel/bulletins/claudecode/auth.rs`: injects OAuth bearer,
  `x-claude-code-session-id`, and Stainless/Claude CLI headers.

## Scope

CCH applies only when the final upstream path is exactly `POST /v1/messages`.

Do not use prefix matching. `POST /v1/messages/count_tokens` is also under the
Claude Messages API, but Anthropic rejects extra `metadata` there, so v2
explicitly skips it.

Before CCH runs, the request has already completed:

1. inbound operation classification;
2. OpenAI/Gemini to Claude Messages protocol conversion;
3. Claude body shaping, such as cache-control hygiene and sampling-parameter
   stripping;
4. optional magic-cache trigger rewriting.

CCH must therefore cover the final body bytes that gproxy itself is about to
send, not the client's original body.

## metadata.user_id

Claude Code's `metadata.user_id` is a JSON string, not an object:

```json
{
  "metadata": {
    "user_id": "{\"device_id\":\"<device>\",\"account_uuid\":\"<account>\",\"session_id\":\"<session>\"}"
  }
}
```

Field sources:

| Field | v2 source |
| --- | --- |
| `device_id` | Stable device id from credential secret; derived from the credential when missing. |
| `account_uuid` | Account id from Claude Code OAuth/cookie login; empty string when missing. |
| `session_id` | Generated by `cch::session_id(...)` and reused as the `x-claude-code-session-id` HTTP header. |

`session_id` is not fully random per request. v2 uses `(device_id, system, first
message, hour)` to select up to 1000 stable slots, then produces a v4-shaped
UUID. This prevents a credential from growing unbounded session ids while
keeping nearby conversations stable.

## Billing Block

`cch::apply` writes a billing text block at the front of `system`:

```text
x-anthropic-billing-header: cc_version=2.1.162.553; cc_entrypoint=cli; cch=00000;
```

If the request already has an old billing block, v2 replaces it in place instead
of adding a second block. This avoids duplicate blocks when proxying an existing
Claude Code request.

If the original `system` is a string, v2 turns it into a Claude block array:

```json
[
  { "type": "text", "text": "x-anthropic-billing-header: ..." },
  { "type": "text", "text": "<original system text>" }
]
```

## Checksum

The algorithm was verified against Claude Code 2.1.162 wire bodies:

```text
cch = xxh64(final_body_bytes_with_cch_00000, seed=0x4d659218e32a3268) & 0xfffff
```

The result is formatted as 5 lowercase hex digits and overwrites the five zeroes
inside `cch=00000;`.

Key points:

- The input is the final JSON bytes, not a single field.
- `model`, `system`, `messages`, `tools`, and `tool_choice` all affect the value.
- HTTP URL, HTTP headers, and OAuth token are not part of the checksum.
- JSON serialization shape matters, so the implementation signs the actual
  `serde_json::to_vec` output.
- After the real value is written, the server can zero it and recompute it.

## Header Relationship

The `session_id` inside the CCH body must equal the HTTP
`x-claude-code-session-id` header.

When the upstream base URL is the default `https://api.anthropic.com` and the
request is `POST /v1/messages`, v2 also generates `x-client-request-id`. Custom
base URLs do not receive that header, matching real SDK behavior.

## Verification

Relevant tests live in `src/channel/bulletins/claudecode/cch.rs` and
`src/channel/bulletins/claudecode/mod.rs`:

- `cch_matches_known_vector` pins the known `b3b78` vector.
- `apply_injects_metadata_and_valid_cch` verifies metadata, billing block, and
  recomputation.
- `apply_replaces_existing_billing_block` verifies re-proxying does not duplicate
  the block.
- `count_tokens_skips_cch_metadata_injection` prevents `count_tokens` from being
  treated as a model path.

When changing the Claude Code channel, run at least:

```bash
cargo test --features full claudecode
```

When upgrading Claude Code, re-check `cc_version`, entrypoint, seed, SDK header
set, and wire-body behavior. Do not trust SDK raw-body logs alone; final wire
body is the source of truth.
