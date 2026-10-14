---
title: CLI 用戶端
description: 把 Codex CLI 和 Claude Code 指向 GPROXY：渠道宣告的廠商服務路由、三種檢視，以及本實例執行的 OAuth issuer。
---

有些廠商 CLI 要說的不只是推理端點。它們還要從廠商的控制面取帳號資料、用量視窗、外掛、
任務和檔案，而且拿不到就不幹活。

因此渠道可以宣告一張**服務路由表**：它認識的控制面路徑，以及每條路徑返回什麼。宿主會在
嘗試資料面之前，把進來的路徑拿去和那張表匹配。

## 怎麼到達它們

控制面路徑是廠商特有的，所以只在 **Provider 或 namespace 掛載點**上匹配。匹配一條服務
路由需要一個渠道，而聚合掛載點沒有點名任何渠道。

```text
https://gproxy.example/codex/backend-api/wham/profiles/me
                       ^^^^^ 名为 "codex" 的 Provider（或 namespace）
```

## 三種檢視

`x-gproxy-view` 說明呼叫方想要什麼。不帶表示 `caller`，而那也是普通成員唯一能要的檢視。

| Header | 回答什麼 |
| --- | --- |
| `x-gproxy-view: caller` | **這個呼叫方**的事實——他們的用量視窗、預算鏈、他們自己的呼叫建立的資源。身份、用量和設定類請求絕不觸達廠商。 |
| `x-gproxy-view: pool` | 同上，但在目標裡的每把憑證上聚合。 |
| `x-gproxy-view: credential:{id}` | 原始轉發。點名憑證自己的鑑權發往上游，廠商的應答原樣回來。 |

誰能要哪一種由歸屬決定，不是由某個開關決定。目標是呼叫方在該 Provider 上**可見的**憑證
集合——和同一把 key 的模型呼叫能花的是同一批——而只有當呼叫方是實例管理員，或者是擁有
該集合中**每一把**憑證的那個組織或團隊的 `Admin` 成員時，他們才是這個目標的管理員。
其餘都是成員，成員只能要 `caller`；`pool` 和 `credential:{id}` 返回 `403`。

"每一把"不是形式主義：管理一個組織，不應該渲染出一個同時包含另一個組織的池。無歸屬的
憑證按構造就沒有管理員——沒有人是"什麼都不是"的 admin 成員——所以在一個憑證全部共享的
單租戶實例上，只有實例管理員能觸達 `pool` 和 `credential`。

**簽發 key 在每種檢視下都被拒絕。** 一把掛在共享訂閱上的長期 API key，正是共享閘道器存在
要避免交出去的那種憑據。

表裡沒有的路徑返回 `404`，除非檢視是 `credential:{id}`。

## 什麼不被計量

廠商服務呼叫**不佔限流租約**——給一次 profile 拉取扣一個視窗，等於花掉呼叫方留給真正花錢
流量的額度——也**不寫 capture 記錄**。引擎不給服務寫上游記錄，因此一條下游記錄將無處可指，
讀起來會像一個沒到達任何上游的請求。

它確實會帶上預算鏈，因為 `caller` 檢視要用它渲染呼叫方自己的視窗。報告一個配額不等於
花掉一個。

廠商服務呼叫與模型請求共用同一套取消機制，覆蓋憑據準備、上游呼叫和 HTTP 回應體讀取。
WebSocket 服務的 token 覆蓋握手階段，已建立的連線由宿主管理。服務仍不佔限流租約，
也不產生模型用量。

## Codex CLI

`codex` 渠道宣告瞭 CLI 在 Responses 之外對 ChatGPT 後端發起的呼叫。wire 事實依據 CLI
自己在 `samples/codex` 下的原始碼：

| 家族 | 路徑 |
| --- | --- |
| 外掛與 MCP | `/backend-api/ps/**` |
| 帳號、設定、用量、任務、環境、遠端控制 | `/backend-api/wham/**` |
| 上傳 | `/backend-api/files/**` |
| 工作區外掛共享 | `/backend-api/public/plugins/**` |
| 啟動時的身份探測 | `GET /v1/user-auth-credential/whoami` |

CLI 用兩種方式定址同一批端點——ChatGPT 主機上是 `/backend-api/wham/**`，Codex API 主機上
是 `/api/codex/**`。兩種拼法，以及裸的 `/codex/**` 和 `/ps/**` 掛載，在查表之前都被歸一
到 `/backend-api/**`，所以用戶端選哪種主機風格不是一個配置問題。

`/backend-api/wham/remote/control/server` 是一個 **WebSocket**，也是唯一一條會升級的服務
路由。Codex 直接拒絕被合成的檢視，因此由該憑證的管理員發出的
`x-gproxy-view: credential:{id}` 是唯一的通路。

表沒有分類的路徑落進 `/backend-api/{*path}` 字首行，原樣轉發，非 2xx 也一樣。

## Claude Code

`claudecode` 渠道宣告瞭 CLI 在 Messages 之外發起的、OAuth 作用域內的 `/api/**` 呼叫：
profile、validate、roles、bootstrap、用量、策略限額、帳號設定、檔案上傳下載、組織聯結器、
外掛與 skill、引導與計費控制、桌面更新跳轉，以及 Claude Design surface。

它們全部解析到 `api.anthropic.com`；`claude.ai` 只提供 cookie 登入，因此渠道在這裡不查它。

在 `caller` 與 `pool` 下，身份、用量與設定類答案由本實例已知的事實合成，其 id 是對
Provider 和呼叫方身份的穩定雜湊。關於那個共享帳號，既不編造也不洩漏：CLI 顯示的帳號、
組織和套餐是 GPROXY 的合成值，不是上游帳號的。

## GPROXY 作為 OAuth issuer

用 OAuth 登入的 CLI 可以登入到**本實例**，而不是登入到廠商。這和 sdk 的 `login()` 是兩件
事，後者是以用戶端身份去講別人的 OAuth：

```text
  Claude Code ──authorize/token──▶ gproxy   （授权服务器）
                                     │
                                     └──login()──▶ OpenAI   （客户端）
```

端點掛在每個掛載點之下，因此用戶端發現的就是它正在對話的那一個：

```text
GET  {mount}/v1/oauth/authorize
POST {mount}/v1/oauth/authorize
POST {mount}/v1/oauth/token
POST {mount}/v1/oauth/device/code
POST {mount}/v1/oauth/revoke
GET  {mount}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{mount}/v1
```

```sh
curl -s http://127.0.0.1:8787/v1/.well-known/oauth-authorization-server
```

```json
{"issuer":"http://127.0.0.1:8787/v1",
 "authorization_endpoint":"http://127.0.0.1:8787/v1/oauth/authorize",
 "token_endpoint":"http://127.0.0.1:8787/v1/oauth/token",
 "device_authorization_endpoint":"http://127.0.0.1:8787/v1/oauth/device/code",
 "revocation_endpoint":"http://127.0.0.1:8787/v1/oauth/revoke",
 "response_types_supported":["code"],
 "grant_types_supported":["authorization_code","refresh_token",
   "urn:ietf:params:oauth:grant-type:device_code"],
 "code_challenge_methods_supported":["S256"]}
```

v3 把它們放在根路徑；v4 移到 `/v1` 之下，讓字首規則與 `/v1/messages` 同構，只需要一份實現。

### 這個 issuer 不讓步的規則

- **永遠 PKCE S256。** `plain` 被拒絕，缺少 challenge 也被拒絕。每個註冊的用戶端都是
  public、沒有 secret 可證明自己，所以 verifier 是洩漏的授權碼與令牌之間唯一的東西。
- **redirect URI 精確匹配。** 不是字首、不是萬用字元、也不是"同源"。每一條更寬鬆的規則都會
  接受一個攻擊者控制的 URL。
- **授權碼、refresh token 與裝置碼一次性**，在一個原子批裡被消費並替換，因此兩次兌換無論
  怎麼交錯都不可能都成功。
- **被重放的授權碼或 refresh token 撤銷整個家族**——授權、它的內部 key，以及它簽發過的
  每一個令牌。沒有辦法把"應答丟了"和"憑據被偷了"區分開，所以一律按洩漏處理。合法用戶端
  重新登入一次；小偷不能。裝置流是例外，因為輪詢用戶端按設計就會重發它的碼。

### 一個 OAuth 呼叫方能做什麼

access token 是使用者交給**別人的二進位制**的一份憑據。除非該用戶端被寫進實例的
`oauth.cliClientIds`，它只能列模型、取模型、數 token、生成、串流和壓縮。其他一律 `403`，
而且這個限制對實例管理員的帳號同樣生效——管理員的帳號恰恰是最不能讓第三方令牌變成管理
憑據的那一個。

把一個用戶端寫進 `cliClientIds`，等於運維者接受它在整個 API 上代表這個使用者說話。

一個 OAuth 呼叫方也不能簽發、輪換、揭示或刪除 key，不能修改帳號密碼。

## 會話親和性

多輪 CLI 在"一次對話停在一把憑證上"時工作得最好。GPROXY 從**實際發出**請求的用戶端讀取
對話標識，絕不從它即將被轉發到的上游讀——一個被轉發到別的渠道的 Claude Code 請求，讀的
仍然是 Claude Code 的欄位。

階梯，第一個非空值勝出：

1. `x-gproxy-session-id`，閘道器自己的 header；
2. `thread-id`、`session-id`、`x-claude-code-session-id`、`x-conversation-id`、
   `x-grok-session-id`；
3. 入站 body 自己的欄位——Responses 的 `client_metadata.thread_id`/`session_id`、
   Claude 在 JSON 編碼的 `metadata.user_id` *內部*的 `session_id`、Gemini 的
   `request.session_id` 與 `request.sessionId`；
4. 對話穩定字首的 sha256 指紋；
5. 請求 id，誠實地標註為兜底，而不是冒充成一個會話。

不會有任何東西在任意 JSON 裡遞迴找名為 `session_id` 的欄位；而請求級、輪次級和快取級
標識——`x-grok-req-id`、`user_prompt_id`、`prompt_cache_key`、`previous_response_id`
——**絕不是**會話。

閘道器 header 在請求發往上游前被摘除，用戶端發來的那一份也被摘除：用戶端不能透過傳送閘道器
用來命名會話的那個 header 去挑別人的對話。缺少會話 header 從不導致請求被拒。
