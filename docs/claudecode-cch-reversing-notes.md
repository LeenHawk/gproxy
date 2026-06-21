# Claude Code CCH 逆向抓取心得

这篇记录的是怎么确认 Claude Code 的 `cch` 算法。重点不是“猜一个 hash”，而是把同一条请求在不同阶段的形态分清楚。

## 结论

Claude Code 2.1.162 的 `cch` 不是 `sha256(first user message)[:5]`。

实际形态是：

```text
cch = xxh64(final_raw_body_with_cch_00000, seed=0x4d659218e32a3268) & 0xfffff
```

然后格式化为 5 位小写 hex，替换 billing header 里的 `cch=00000;`。

因为输入是最终 raw body，所以 `model`、`system`、`messages`、`tools`、`tool_choice` 等字段都会影响结果。URL、HTTP headers、OAuth token 不参与计算。

## 关键经验

第一步要先接受一个事实：Claude Code 的可读 JS bundle 里看到的 body，不一定就是最终上网的 body。

这次最容易误判的地方是 `OTEL_LOG_RAW_API_BODIES`。它能抓到 SDK 层 raw body，但那里仍然是：

```text
x-anthropic-billing-header: ...; cch=00000;
```

真正的 `cch` 是更晚才被改写的。也就是说，SDK raw-body 日志只能证明“placeholder 在 SDK 层存在”，不能证明最终 wire body 也是 `00000`。

要分三层看：

1. **bundle / 可读源码层**：找谁生成 billing header、占位符、版本号、entrypoint。
2. **SDK raw body 层**：确认请求 JSON 在 SDK 发出前大概是什么样。
3. **wire body 层**：看实际发到 `api.anthropic.com` 的 body，这一层才是判断 `cch` 的准绳。

## 抓取路径

### 1. 先找 billing header 生成点

在 bundle 字符串里搜：

```text
x-anthropic-billing-header
cch=00000
cc_version
cc_entrypoint
```

这一步能确认 Claude Code 先生成的是占位 header，而不是一开始就算出最终值。

### 2. 用 raw body 日志确认 SDK 层

用 `OTEL_LOG_RAW_API_BODIES` 抓 SDK 层请求体。看到 `cch=00000` 时不要急着下结论，它只说明这个阶段还没改写。

如果 raw body 和 wire body 不一致，优先相信 wire body。

### 3. 抓 wire body

用本地 MITM / 调试代理抓 `POST /v1/messages` 的实际请求体。注意要脱敏：

- `Authorization`
- OAuth token
- cookie
- 用户真实 prompt

wire body 里能看到 `cch` 已经从 `00000` 变成 5 位 hex，例如 `f4d23` 这类值。

### 4. 做最小变量实验

不要只跑一个 prompt。至少改这些点：

- 改 user message
- 改 model
- 改 `max_tokens`
- 加/删 `tools`
- 调整 JSON 字段顺序或序列化形态
- 固定 `CLAUDE_CODE_SESSION_ID`

这一步的目的不是马上推出公式，而是排除错误假设。

如果固定 session 但 `cch` 仍随 body 变化，说明它不是稳定 session token。如果只改 `tools` 也会变，说明它不是单纯的 message text hash。

## 二进制反编译路径

确认 wire body 和 SDK body 的差异后，再回到二进制找改写点。这里不能只看 JS bundle，因为 Claude Code 是 Bun 打包的 ELF，可读 JS 只是其中一层。

先确认入口：

```bash
readlink -f /home/linhuan/.local/bin/claude
file /home/linhuan/.local/share/claude/versions/2.1.162
readelf -S /home/linhuan/.local/share/claude/versions/2.1.162
```

当时看到的是：

- `/home/linhuan/.local/bin/claude` 指向 `versions/2.1.162`
- 目标是 ELF
- ELF 里有 `.bun` section
- `.bun` section 里有可读 JS bundle，也有 native / bytecode 相关内容

所以分析要分两段：

1. 先在 `.bun` / strings 里找可读逻辑，确认 placeholder 是谁生成的。
2. 再进反汇编里找真正改写 `cch=00000` 的 native 路径。

### 1. 用字符串锚点缩小范围

有效线索是：

```text
cch=00000
/v1/messages
"system":[
x-anthropic-billing-header
xxhash / XXH64 相关常量
```

可读 bundle 里能看到类似 `Pt$(H)` 的 helper，生成：

```text
x-anthropic-billing-header: cc_version=2.1.162.<suffix>; cc_entrypoint=sdk-cli; cch=00000;
```

这一步只说明 billing header 的占位形式，不说明最终算法。

### 2. 从 `cch=00000` 交叉引用进反汇编

在反汇编工具里围绕 `cch=00000`、`/v1/messages`、`"system":[` 找 xref。关键不是看字符串本身，而是看谁在最终发送前扫描 body、替换这 5 个字符。

Claude Code 2.1.162 里看到的关键形态是：

1. 找到 body 里的 `cch=00000`
2. 初始化 XXH64 state
3. 对完整 body bytes 做 hash
4. finalize
5. 取低 20 bit
6. 写回 5 个 hex 字符

当时的关键位置大致是：

```text
0x2e06693  附近处理 / 搜索 cch=00000
0x2e06757  初始化 XXH64 state
0x2e067b6  把完整 body bytes 喂给 hash
0x2917fe0  XXH64 finalize，能看到 XXH64 prime 相关操作
0x2e06878  写回 5 位 hex
0x2e0687e  写回完成后的继续发送路径
```

这些 offset 只对 Claude Code 2.1.162 有意义；换版本要重新找，不能硬编码到文档外的实现里。

### 3. 识别 XXH64，而不是靠名字

二进制里不一定有友好的函数名。判断它是 XXH64，主要看三类证据：

- hash state 的初始化和更新形态符合 XXH64
- finalize 阶段能看到 XXH64 prime / avalanche 风格的混合
- 用同一份 wire body 回算，结果能精确匹配最终 `cch`

第三点最重要。反编译只能给候选算法；能不能回算匹配 wire body，才决定结论是否成立。

### 4. 注意触发条件

这个改写路径不是对任意 JSON 都触发。2.1.162 里至少会检查类似这些条件：

- 请求目标是 `/v1/messages`
- body 里有 `"system":[`
- system 前部能找到 billing header
- billing header 里有 `cch=00000`

所以用 pretty JSON、字段顺序不同、system 不是数组，可能不会走到同一条 native 改写路径。验证算法时要尽量使用真实客户端发出的 compact raw body。

## 验证方式

验证时用同一份最终 body，把 billing header 里的真实值替换回：

```text
cch=00000;
```

然后计算：

```text
xxh64(body_bytes, 0x4d659218e32a3268) & 0xfffff
```

输出按 5 位小写 hex 格式化。如果和 wire body 的 `cch` 一致，算法才算确认。

## 容易踩坑

- 不要把 SDK raw body 当成 wire body。
- 不要只用一个 prompt 推公式。
- 不要只 hash 第一条 user message。
- 不要忽略 `tools`，它在最终 body 里，会影响 `cch`。
- 不要把 seed 当成永久常量；它可能随 Claude Code 版本变化。
- 不要用 pretty JSON 验证最终值；hash 对原始 bytes 敏感。

## 对 gproxy 的含义

如果要在 `claudecode` 渠道实现这个逻辑，签名必须放在所有 transform / process / rule 改写之后、请求发出之前。

实现时应对最终要发送的 body 做签名，而不是对解析后的某个字段、首条消息或规范化 JSON 做签名。

## English

# Claude Code CCH Reverse-Engineering Notes

This note records how the Claude Code `cch` algorithm was confirmed. The main
lesson is not "guess a hash"; it is separating the same request at different
layers.

## Conclusion

Claude Code 2.1.162 `cch` is not `sha256(first user message)[:5]`.

The observed form is:

```text
cch = xxh64(final_raw_body_with_cch_00000, seed=0x4d659218e32a3268) & 0xfffff
```

The result is formatted as five lowercase hex digits and replaces
`cch=00000;` in the billing header.

Because the input is the final raw body, `model`, `system`, `messages`, `tools`,
`tool_choice`, and similar fields all affect the result. URL, HTTP headers, and
OAuth token do not.

## Key Lesson

The readable JS bundle inside Claude Code does not necessarily show the final
body that reaches the network.

The easy trap is `OTEL_LOG_RAW_API_BODIES`: it captures the SDK-layer raw body,
where the header may still contain:

```text
x-anthropic-billing-header: ...; cch=00000;
```

The real `cch` is patched later. SDK raw-body logs prove only that the
placeholder exists at that layer; they do not prove the final wire body still
contains `00000`.

Separate three layers:

1. **Bundle / readable source layer**: find who creates the billing header,
   placeholder, version, and entrypoint.
2. **SDK raw body layer**: confirm the approximate JSON before the SDK sends it.
3. **Wire body layer**: inspect the actual body sent to `api.anthropic.com`; this
   layer decides the algorithm.

## Capture Path

### 1. Find The Billing Header Generator

Search bundle strings for:

```text
x-anthropic-billing-header
cch=00000
cc_version
cc_entrypoint
```

This confirms Claude Code first creates a placeholder header instead of knowing
the final checksum upfront.

### 2. Use Raw Body Logs For The SDK Layer

Use `OTEL_LOG_RAW_API_BODIES` to capture the SDK-layer body. Seeing
`cch=00000` there is not enough; it only means the rewrite has not happened yet.

If raw body and wire body differ, trust the wire body.

### 3. Capture The Wire Body

Use local MITM / debug proxy to capture the real `POST /v1/messages` body.
Redact:

- `Authorization`;
- OAuth token;
- cookies;
- real user prompts.

The wire body should show `cch` changed from `00000` to a five-digit hex value.

### 4. Run Minimal Variable Experiments

Do not rely on one prompt. Change at least:

- user message;
- model;
- `max_tokens`;
- adding/removing `tools`;
- JSON field order or serialization shape;
- fixed `CLAUDE_CODE_SESSION_ID`.

The goal is to rule out bad hypotheses. If fixed session still changes with the
body, it is not a stable session token. If changing `tools` changes `cch`, it is
not a hash of only message text.

## Binary Decompilation Path

After confirming wire body differs from SDK body, go back to the binary. The JS
bundle alone is not enough because Claude Code is a Bun-packed ELF.

First confirm the entry:

```bash
readlink -f /home/linhuan/.local/bin/claude
file /home/linhuan/.local/share/claude/versions/2.1.162
readelf -S /home/linhuan/.local/share/claude/versions/2.1.162
```

The observed target:

- `/home/linhuan/.local/bin/claude` points to `versions/2.1.162`;
- the target is ELF;
- it contains a `.bun` section;
- `.bun` contains readable JS bundle data plus native/bytecode-related content.

Analyze in two steps:

1. Search `.bun` / strings for readable logic and confirm who creates the
   placeholder.
2. Search disassembly for the native path that replaces `cch=00000`.

### 1. Use String Anchors

Useful anchors:

```text
cch=00000
/v1/messages
"system":[
x-anthropic-billing-header
xxhash / XXH64 constants
```

The readable bundle shows a helper creating a string like:

```text
x-anthropic-billing-header: cc_version=2.1.162.<suffix>; cc_entrypoint=sdk-cli; cch=00000;
```

This only proves placeholder shape, not the final algorithm.

### 2. Cross-Reference `cch=00000`

In a disassembler, search xrefs around `cch=00000`, `/v1/messages`, and
`"system":[`. The important code path scans the final body and replaces five
characters before sending.

Claude Code 2.1.162 showed this shape:

1. find `cch=00000` in body;
2. initialize XXH64 state;
3. feed full body bytes to the hash;
4. finalize;
5. take low 20 bits;
6. write five hex characters back.

Approximate offsets observed in that version:

```text
0x2e06693  search/handling around cch=00000
0x2e06757  initialize XXH64 state
0x2e067b6  feed full body bytes
0x2917fe0  XXH64 finalize, with XXH64-prime-style operations
0x2e06878  write five hex digits
0x2e0687e  continue send path
```

These offsets only apply to Claude Code 2.1.162. Re-find them for other
versions; do not hard-code offsets outside these notes.

### 3. Identify XXH64 By Behavior

The binary may not expose friendly function names. The evidence for XXH64 is:

- state init and update shape match XXH64;
- finalize phase has XXH64 prime / avalanche-style mixing;
- recomputing on the same wire body exactly matches the final `cch`.

The third point matters most. Decompilation gives candidate algorithms; exact
wire-body recomputation confirms the answer.

### 4. Watch Trigger Conditions

The rewrite path does not trigger for arbitrary JSON. In 2.1.162 it checks
conditions like:

- target is `/v1/messages`;
- body contains `"system":[`;
- a billing header is near the front of system;
- billing header contains `cch=00000`.

Pretty JSON, different field order, or non-array `system` may not hit the same
native rewrite path. Verify with compact raw bodies from the real client.

## Verification

To verify, take the same final body and replace the real billing header value
back to:

```text
cch=00000;
```

Then compute:

```text
xxh64(body_bytes, 0x4d659218e32a3268) & 0xfffff
```

Format as five lowercase hex digits. If it matches the wire body `cch`, the
algorithm is confirmed.

## Pitfalls

- Do not treat SDK raw body as wire body.
- Do not derive the formula from one prompt.
- Do not hash only the first user message.
- Do not ignore `tools`; they are in the final body and affect `cch`.
- Do not assume the seed is permanent; it may change across Claude Code versions.
- Do not use pretty JSON to verify the final value; the hash is byte-sensitive.

## Meaning For gproxy

In the `claudecode` channel, signing must happen after all transform / process /
rule mutations and immediately before the upstream request is sent.

The implementation should sign the final body bytes to be sent, not a parsed
field, first message, or normalized JSON abstraction.
