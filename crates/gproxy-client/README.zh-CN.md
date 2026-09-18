# gproxy-client

[English](README.md) | 简体中文

GPROXY v4 原生出站 Client 复用库。默认启用 `reqwest` feature；`wreq` feature
提供 TLS／HTTP 指纹模拟，可同时启用两者。该 crate 依赖 protocol 只为定义传输契约
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
这里不另造缓冲或协议适配层。WASM 目前只暴露配置类型，尚未实现 Fetch 接线。

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

store 保存具名的 `ConnectionProfile`；宿主按凭证 → Provider → 全局顺序选择
第一个非空 `connection_profile_id`，加载整份配置，再构造 `ConnectionConfig`。
全部未选时使用默认 reqwest／直连；引用的记录不存在时报错，不回退。
整份替换，不按字段合并，连接配置之间没有继承。

| store 字段 | client 配置 |
|---|---|
| `backend` | `Backend::Reqwest`／`Backend::Wreq` |
| `proxy_mode = direct/system` | `ProxyConfig::Direct`／`System`，`proxy_url` 必须为空 |
| `proxy_mode = explicit`、`proxy_url` | `ProxyConfig::Explicit { url }`，必须提供 URL |
| `emulation` JSON／空 | 反序列化成 `EmulationConfig`／`None` |
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
  "connect_timeout_ms": 10000,
  "pool_idle_timeout_ms": 90000,
  "pool_max_idle_per_host": 32
}
```

指纹使用 wreq-util 的具名 TLS／HTTP 预设，不支持任意自定义 TLS 参数 JSON。
`http2` 表示应用预设的 HTTP/2 参数，设为 false 不等于禁止 HTTP/2；`headers`
控制预设请求头。profile／platform 使用 wreq-util 的 serde 名称，未知名称在构造 wreq Client 时报错。
指纹仅对 wreq 生效。未知字段或未编译的后端均明确报错。正常证书校验始终开启。
配置不做预校验；底层构造错误返回给调用方。

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
