# gproxy-client

[English](README.md) | 简体中文

GPROXY v4 原生出站 Client 复用库。默认启用 `reqwest` feature；`wreq` feature
提供 TLS／HTTP 指纹模拟；`reqwest-native` feature 提供走平台原生 TLS 的 reqwest 0.12
（Linux 上为 OpenSSL、macOS 为 Secure Transport、Windows 为 SChannel，即 Codex CLI 的
HTTP 栈，`Backend::ReqwestNative`），三者可同时启用。该 crate 依赖 protocol 只为定义传输契约
`OutboundClient`（`Client` 已实现：send 返回流式响应，connect 返回双向连接或被拒的握手响应；
wreq 后端被拒时不保留 body），不依赖 store，
不负责选路、凭证选择或数据库读取。

```rust,no_run
use gproxy_client::{Client, ClientPool, ConnectionConfig};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let pool = ClientPool::default(); // 宿主长期保留
let client = pool.get(&ConnectionConfig::default()).await?;
if let Client::Reqwest(http) = client.as_ref() {
    let response = http.get("https://example.com").send().await?;
    println!("{}", response.status());
}
# Ok(())
# }
```

`Client` 暴露对应后端的原生请求／流式 multipart API。reqwest 的 WS 使用重导出的
`reqwest_websocket` 扩展，wreq 使用原生 WebSocket API。
这里不另造缓冲或协议适配层。

## wasm32

wasm32 上有同样的 `Client`、`ClientPool` 与 `OutboundClient`，按 feature 而不是按平台
crate 组织：

| feature | 传输 | body | WebSocket |
|---|---|---|---|
| `fetch` | 经 web-sys 调用 JS 宿主的全局 `fetch`（Cloudflare Workers、Deno、Netlify Edge、浏览器） | 双向流式（`duplex: half`） | 需要 `workers` 或宿主 client |
| `workers`（隐含 `fetch`） | 同上，加 Cloudflare Workers 升级：带 `Upgrade: websocket` 的 `fetch`，取 `response.webSocket` 后 `accept()` 暴露为协议 socket | 流式 | 支持，且带上游鉴权头 |
| `reqwest`（默认） | reqwest 自带的 Fetch 兜底 | 请求体先缓冲 | 不支持 |

同时启用时 `fetch` 优先。profile 仍用于选择 client，但代理、TLS 指纹和连接池
是原生概念，这里忽略，由 JS 宿主决定。宿主自己有传输时（Workers 的 `Fetch` 带 service
binding 与 `cf` 选项、Deno 的 `createHttpClient` 带代理或 CA、其他目标上的 `wasi:http`
封装）实现 `OutboundClient`，用 `ClientPool::with_client` 或按 profile 的
`ClientPool::with_factory` 注入，core 就不会碰内置传输。`wasm32-wasip2` 没有 JS `fetch`，
不在这些 feature 覆盖范围内。

feature `libsql`（任意目标）让 `Client` 充当 gproxy-seaorm 的 libSQL／Turso 连接的 HTTP
一段，数据库与其余出站共用传输。

## Multipart 与 WebSocket

两种后端都开启 `multipart`／`stream`。通过缓存的句柄使用对应的 `multipart::Form`
和 `multipart::Part::stream(Body::wrap_stream(...))`，支持长度未知的流式文件上传。

RFC 6455 WS／WSS 使用 `pool.get_websocket(&config).await?` 获取句柄。保留相同的
代理、TLS／emulation 和其他策略，但强制 HTTP/1.1（包括 TLS ALPN），与普通 HTTP
分别缓存，避免 WSS 协商到 HTTP/2 后无法完成 HTTP/1.1 Upgrade 握手。

- reqwest：导入 `gproxy_client::reqwest_websocket::Upgrade`，调用
  `http.get(url).bearer_auth(token).upgrade().protocols(["protocol"]).send().await?`。
- wreq：调用 `http.websocket(url).bearer_auth(token).protocols(["protocol"]).send().await?`。

检查握手响应后，通过 `into_websocket().await?` 获得双向文本／二进制流，并支持关闭帧。
升级后的 socket 属于该 WS 会话；Client 复用不等于复用已有逻辑 WS 会话。
清理 Client 缓存不会中断已升级的 WS 流。

## 配置选择

store 保存具名的 `ConnectionProfile`；宿主按凭证 → Provider 顺序选择第一个非空
`connection_profile_id`，都没有时用渠道自带的 `default_connection()`，再退到全局
默认 profile，加载整份配置，再构造 `ConnectionConfig`。
全部未选时使用默认 reqwest／直连；引用的记录不存在时报错，不回退。
整份替换，不按字段合并，连接配置之间没有继承。

| store 字段 | client 配置 |
|---|---|
| `backend` | `Backend::Reqwest`／`Backend::Wreq`／`Backend::ReqwestNative`（`reqwest_native`） |
| `proxy_mode = direct/system` | `ProxyConfig::Direct`／`System`，`proxy_url` 必须为空 |
| `proxy_mode = explicit`、`proxy_url` | `ProxyConfig::Explicit { url }`，必须提供 URL |
| `emulation` JSON／空 | 反序列化成 `EmulationConfig`（`kind: preset` 或 `kind: custom`；旧版本的扁平预设对象仍可加载）／`None` |
| `gzip`、`brotli`、`deflate`、`zstd` | 独立自动解压开关，默认 false |
| `redirect_max_hops` | 0 不跟随；正数限制最大重定向跳数 |
| `retry` | `never`（默认）或 `default`（后端原生重试策略） |
| 三个超时／池容量字段 | 同名字段直接映射 |

继承、映射和数据库调用仍待未来宿主接线，
store 当前仅为 entity 草案。代理 URL 可能包含认证信息，序列化配置和 entity Debug
输出不应作为普通用户可见资料或日志内容。

有效配置示例：

```json
{
  "backend": "wreq",
  "proxy": { "mode": "explicit", "url": "socks5h://127.0.0.1:1080" },
  "emulation": {
    "kind": "preset",
    "profile": "chrome_133",
    "platform": "linux",
    "http2": true,
    "headers": false
  },
  "gzip": true,
  "brotli": true,
  "deflate": true,
  "zstd": true,
  "redirect_max_hops": 5,
  "retry": "default",
  "pool_idle_timeout_ms": 90000,
  "pool_max_idle_per_host": 32
}
```

`preset` 指纹使用 wreq-util 的具名 TLS／HTTP 预设。`http2` 表示应用预设的 HTTP/2
参数，设为 false 不等于禁止 HTTP/2；`headers` 控制预设请求头。profile／platform 使用
wreq-util 的 serde 名称，未知名称在构造 wreq Client 时报错。

`custom` 指纹是一份显式的 `Fingerprint`，渠道用它承载捕获到的 CLI 身份。所有字段可选，
未设置的字段保持 wreq（BoringSSL）默认：`alpn`（`http1`／`http2`／`http3`，按提供顺序）、
`min_tls`／`max_tls`（`tls10`..`tls13`）、BoringSSL 语法的 `cipher_list`、`curves_list`、
`sigalgs_list`、`preserve_tls13_cipher_list`、`grease`、`ocsp_stapling`、
`signed_cert_timestamps`、`http2`（`enable_push`、`initial_window_size`、
`initial_connection_window_size`、`max_frame_size`、`max_header_list_size`、
`header_table_size`、`max_concurrent_streams`、`pseudo_header_order`、`settings_order`、
`headers_priority`）与 `headers`（有序的 `[name, value]` 默认请求头，保留原始大小写）。

```json
{
  "kind": "custom",
  "alpn": ["http1"],
  "min_tls": "tls12",
  "cipher_list": "TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256",
  "grease": false,
  "http2": { "enable_push": false, "initial_window_size": 2097152 }
}
```

wreq 应用完整伪装。reqwest 应用自定义指纹中的默认请求头、支持的 TLS 版本、
HTTP/1 或 HTTP/2 选择、流与连接窗口大小、帧大小和请求头列表大小。密码套件、GREASE、
TLS 扩展顺序及 HTTP/2 顺序等不支持的项跳过。具名 wreq-util 预设仅在 wreq 上可用；
跨后端预设使用 `kind: custom`。显式请求头优先于指纹默认请求头。
edge/Fetch 应用宿主允许的默认请求头，重定向次数为 0 时不跟随，大于 0 时交给宿主跟随；
实际跳转上限由宿主决定。未知字段或未编译的后端仍明确报错。正常证书校验始终开启。
配置不做预校验；底层构造错误返回给调用方。

`Backend::ReqwestNative` 按 Codex CLI 的方式构造 reqwest 0.12：原生 TLS、h2 默认
SETTINGS、不解压、没有重试层，因此跳过解压开关和 `retry: default`；原生 TLS API 不支持的 TLS 1.3 版本选择也会跳过。它的
WebSocket 变体（池的 `get_websocket`）是 rustls 的 `reqwest` client，与 CLI 自己的
WebSocket 走 rustls 一致，所以该 feature 隐含 `reqwest`。Linux 上 OpenSSL 从源码构建
（`openssl-src`，构建需要 `perl` 与 `make`），同时启用 `wreq` 时 BoringSSL 以符号前缀方式
构建（需要 `nm`／`objcopy`）：两套 TLS 库的归档名与符号名相同，系统 `libssl.so` 无法与
wreq 的静态 BoringSSL 一起链接。CLI 自己在 glibc 上链接系统 OpenSSL，从源码构建的
OpenSSL 3 的 ClientHello 与之相同。该 feature 仅原生目标可用。

## 缓存与生命周期

缓存 key 包含全部有效连接参数及 WS 的 HTTP/1.1 覆盖，不含配置 ID、名称、version、目标 URL 或 API Key。
代理 URL 必须解析并归一化为 authority 形式，统一主机名、默认端口和末尾斜杠写法；解析失败返回错误。
任意一个空闲连接池参数为零时，两个参数均归零。
代理认证、完整模拟参数、解压、重定向和重试设置都参与比较。API Key 按请求设置，
默认不启用 Cookie 存储或连接绑定的账号身份。

同 key 并发未命中时只构建一次，构建工作放到 Tokio blocking pool；失败不缓存。
默认缓存 256 个 Client、空闲 10 分钟淘汰；每个 Client 每个 host 最多保留 32 条
空闲连接、90 秒回收。这些都不是请求并发限制。Moka 在访问时维护容量／过期；
宿主可定时调用 `prune()`，使完全空闲的缓存也及时释放引用。

参数变化自然命中新 key。淘汰或 `clear()` 只释放缓存持有的引用，不中断已取得的
Client／响应流。系统代理由后端构造时读取；运行期间应保持环境／系统代理稳定，
变更后清理缓存再获取 Client。

两个后端默认关闭自动重定向、重试、解压，均可通过连接配置开启。
`gzip`／`brotli`／`deflate`／`zstd` 独立控制自动解压与 Accept-Encoding 自动协商；
模拟预设或请求中已有的 Accept-Encoding 优先于自动生成。开启解压后，后端同时移除
Content-Encoding／Content-Length；关闭时保留编码 bytes／headers。wreq 先应用
emulation，再应用明确的解压开关。

`redirect_max_hops > 0` 按指定上限跟随重定向，超过上限报错。
`retry = "default"` 恢复库自带的安全协议层重试（目前最多两次），不添加 HTTP 状态码
重试规则；`never` 关闭这些重试。应用层的路由尝试次数仍是独立策略。非 2xx 保留为响应。
只设置连接超时，不设置整条响应的总超时，避免截断长时间生成流。
取消、单请求截止时间及并发限制由调用方管理。

验证命令：`cargo test -p gproxy-client --all-features`。本地回环测试覆盖 TCP 复用、
HTTP 代理认证／HTTPS CONNECT、不同 wreq 预设的 TLS ClientHello 差异、缓存 key／
并发构建、重定向跳数、独立解压开关、清理缓存后的响应读取、流式 multipart 上传，
以及直连／HTTP 代理下 WS 文本／二进制／子协议／关闭帧。ClientHello 捕获不等于
完整 TLS／浏览器指纹等价或真实供应商接入验证。
