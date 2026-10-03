---
title: "路由與端點"
description: v4 完整的入站表、掛載點語法、OAuth 與管理路由、WebSocket surface、解析與失敗轉移預算。
---

下面的一切就是原生宿主的路由表，而 Workers 宿主掛載的是同一張。`gproxy-protocol` 刻意
不宣告任何路徑——哪個 URL 承接哪個操作是入口層擁有的 HTTP 約定——因此本頁就是那一層的表。

## 閘道器自己的路由

四條，其餘全部落到資料面。這和通常的安排正好相反，而且是刻意的：閘道器自己的路由是一份短
而已知的清單，別的都是本實例轉發的、屬於別人的 API。新增一個上游 surface 不該需要在這裡
新增一條路由。

| 方法 | 路徑 | 是什麼 |
| --- | --- | --- |
| `GET` | `/healthz` | 存活與已釋出的配置 revision；**不需要認證** |
| `GET` | `/publications/{id}` | 一份已釋出的 body。id **就是**憑據，因此不問 key |
| — | `/admin/api/…` | 運維面 |
| — | `/portal/api/…` | 終端使用者自己的面 |
| — | 其餘一切 | 入站兜底 |

```sh
curl -s http://127.0.0.1:8787/healthz
```

```json
{"revision":2,"status":"ok"}
```

`/healthz` 讀已釋出的快照，既不碰資料庫也不碰 cache，因此負載均衡器輪詢它不會自己變成
壓垮它的那份負載。

## 入站的順序

1. **CORS 與用戶端地址。** 預檢在這裡應答、絕不轉發——它問的是本實例接受什麼，上游對此
   沒有意見。用戶端地址在任何東西能記錄它之前，按
   [可信代理規則](/zh-tw/reference/configuration/#可信代理規則)解析。
2. **掛載點**——路徑裡哪一段是本閘道器的。
3. **OAuth issuer**，在 `{mount}/v1/oauth/…`。排在資料面之前，因為這些路徑是*本實例*的
   而不是某個上游的，而且它們用另一套錯誤信封應答。
4. **某個渠道的廠商服務路由**——Codex 的 `/backend-api/…`、Claude Code 的 `/api/…`。
   只在 Provider 或 namespace 掛載點上：匹配它需要一個渠道，而聚合掛載點沒有點名渠道。
5. **資料面**，見下。
6. **console**，帶 SPA 兜底，受配置開關控制。

走到盡頭的一切都是 404。

## 資料面

請求體在做任何事之前（包括鑑權）先受 `settings.maxRequestBodyBytes` 約束：預設 50 MiB，
檔案上傳用 `maxUploadBodyBytes`，預設 512 MiB。

### 內容生成

| 方法 | 路徑 | 操作 / 方言 |
| --- | --- | --- |
| `POST` | `/v1/responses` | `generate_content` / `openai` |
| `POST` | `/v1/chat/completions` | `generate_content` / `openai_chat` |
| `POST` | `/v1/messages` | `generate_content` / `claude` |
| `POST` | `/v1/messages/count_tokens` | `count_tokens` / `claude` |
| `POST` | `/v1beta/models/{model}:generateContent` | `generate_content` / `gemini` |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | `stream_generate_content` / `gemini` |
| `POST` | `/v1beta/models/{model}:countTokens` | `count_tokens` / `gemini` |
| `POST` | `/v1beta/models/{model}:embedContent` | `create_embedding` / `gemini` |
| `POST` | `/v1beta/models/{model}:batchEmbedContents` | `batch_create_embedding` / `gemini` |

前三條在 body 說 `"stream": true` 時變成 `stream_generate_content`。它們是**兩個不同的
操作**——不同的規則、不同的結算——所以這個標誌在入口就被讀取，而不是留給渠道去發現。
Gemini 把它寫在路徑裡，這也是那個方言有兩行的原因。

### 模型

| 方法 | 路徑 | 操作 / 方言 |
| --- | --- | --- |
| `GET` | `/v1/models` | `list_models` / `openai`，再 `claude` |
| `GET` | `/v1/models/{id}` | `get_model` / `openai`，再 `claude` |
| `GET` | `/v1beta/models` | `list_models` / `gemini` |
| `GET` | `/v1beta/models/{id}` | `get_model` / `gemini` |

### 其餘

| 方法 | 路徑 | 操作 / 方言 |
| --- | --- | --- |
| `POST` | `/v1/responses/compact` | `compact_content` / `openai` |
| `POST` | `/v1/embeddings` | `create_embedding` / `openai` |
| `POST` | `/v1/moderations` | `create_moderation` / `openai` |
| `POST` | `/v1/rerank` | `rerank` / `openai` |
| `POST` | `/v1/conversations` | `create_conversation` / `openai` |
| `POST` | `/v1/images/generations` | `create_image` / `openai` |
| `POST` | `/v1/images/edits` | `edit_image` / `openai` |
| `POST` | `/v1/audio/speech` | `create_speech` / `openai` |
| `POST` | `/v1/audio/transcriptions` | `create_transcription` / `openai` |
| `POST` | `/v1/audio/translations` | `create_translation` / `openai` |

### 檔案

| 方法 | 路徑 | 操作 / 方言 |
| --- | --- | --- |
| `GET` `POST` | `/v1/files` | `list_files` / `create_file`，`openai` 再 `claude` |
| `GET` `DELETE` | `/v1/files/{id}` | `retrieve_file` / `delete_file` |
| `GET` | `/v1/files/{id}/content` | `retrieve_file_content` |
| `GET` `POST` | `/v1beta/files` | Gemini 的寫法 |
| `POST` | `/upload/v1beta/files` | `create_file` / `gemini` |
| `GET` `DELETE` | `/v1beta/files/{id}` | `retrieve_file` / `delete_file`，`gemini` |
| `GET` | `/v1beta/files/{id}:download` | `retrieve_file_content` / `gemini` |

### 影片

| 方法 | 路徑 | 操作 |
| --- | --- | --- |
| `GET` `POST` | `/v1/videos` | `list_videos` / `create_video` |
| `GET` `DELETE` | `/v1/videos/{id}` | `retrieve_video` / `delete_video` |
| `GET` | `/v1/videos/{id}/content` | `download_video_content` |

只有 Sora 有的操作——remix、edit、extend、characters——**刻意缺席**。沒有第二家廠商提供
它們，等有第二家的時候它們會回來。

### 誰能存取檔案與影片

檔案和影片的 id 是上游自己的，而一個上游憑證通常由許多呼叫方共用，所以閘道自己記錄誰建立了
什麼。上傳或建立影片成功後，閘道把它記在呼叫方名下（同一使用者的所有金鑰和主控台工作階段算作
同一呼叫方；每個 OAuth 授權各算一個呼叫方），並記下處理該請求的憑證。

- 取得、下載、輪詢或刪除某個 id 時，除非是該呼叫方經閘道建立的，否則一律回傳 **404**——
  屬於別人的 id 和不存在的 id 得到同樣的答覆。請求會送往持有該資源的憑證。
- 列表只顯示呼叫方自己的項目。分頁游標（`has_more`、`last_id`、`nextPageToken`）原樣透傳，
  因此一頁的項目可能少於 `limit`，甚至在還有後續頁時為空；請一直翻頁，直到上游表示沒有更多。
- 在此檢查出現之前建立的、或直接在上游建立的檔案和影片，無法透過閘道存取。
- Gemini 的*可續傳*上傳（google-genai SDK 的預設方式）照常可用：start 回傳的
  `x-goog-upload-url` 是綁定到呼叫方的閘道 URL，位元組會在同一憑證上轉送到上游工作階段，上傳完成的
  檔案歸呼叫方所有。其他呼叫方使用該 URL 會得到 **404**。部署在代理之後時，請設定
  `public_base_url`（或轉送 `Host` 和受信任的 `X-Forwarded-Proto`），讓該 URL 指向用戶端能存取的位址。
- 同樣的規則也適用於其他請求內部引用的檔案：Chat、Responses 或 Claude 訊息裡的 `file_id`
  （包括 code interpreter 容器的 `file_ids`）、指向 Files API 檔案的 Gemini
  `fileData.fileUri`、影片的 `input_reference`。不是呼叫方上傳的檔案會在送出任何請求之前
  回傳 **404**，請求會在持有這些檔案的憑證上執行；分別上傳到兩個不同憑證的檔案不能在同一
  請求中使用（**400**）。multipart 表單同樣檢查（`file_id`、`file_ids[]`、
  `input_reference[file_id]` 等欄位）。
- 在 Responses WebSocket 上，新的 response 會在持有其引用檔案的憑證上建立上游連線；續接
  （`previous_response_id`）留在原帳號，因此引用其他憑證上的檔案會收到 `error` 事件——請開始新的
  response 來使用它們。
- 在 Realtime 或 Gemini Live 套接字上，引用了呼叫方在該套接字憑證上並不擁有的檔案的用戶端訊息
  不會被轉送：Realtime 回傳 `error` 事件（`file_not_found`），Gemini Live 以代碼 `1008` 關閉。

### Realtime 與套接字

| 方法 | 路徑 | 操作 / 方言 |
| --- | --- | --- |
| `POST` | `/v1/realtime/calls` | `create_realtime_call` / `openai` |
| `POST` | `/v1/live` | `create_realtime_call` / `openai` |
| `GET` | `/v1/realtime` | `connect_realtime` / `openai`——升級 |
| `GET` | `/v1/live` | 同上，WebRTC 寫法 |
| `GET` | `/v1/live/{call_id}` | 把 call 放在路徑裡的續接 |
| `GET` | `/v1/responses` | `generate_content` / `openai_responses_websocket` |
| `GET` | `/ws/v1beta/BidiGenerateContent` | `connect_realtime` / `gemini`——Gemini Live |

Responses 的 HTTP POST 與 WebSocket GET 共用 `/v1/responses`。握手先認證呼叫者，
再由每條 `response.create` 中的模型完成權限、選路和生成限流。不同 `stream_id` 可以併發；
新的回應鏈可以重新選擇目標，攜帶 `previous_response_id` 的續接保持原目標繫結。
空閒 Responses 連線不佔用生成併發名額。

原生 Responses WS 轉發上游控制事件。對 Responses HTTP、Chat、Claude、Gemini 的橋接
支援本地上下文預熱、中斷、steering 後繼，以及在 HTTP 生成段之間吸收 injection 輸入。
本地預熱不代表上游快取預填充。`store:false` 歷史僅在用戶端連線內保留；持久化續接儲存
隔離後的歷史和目標繫結。已經開始或傳送結果不確定的生成不會換目標重放。

`POST /v1/realtime/calls` 是一個攜帶 SDP offer 的 HTTP multipart 請求，而續接它的握手
**完全不帶 body**。活過升級的是 call id——在路徑裡，或在 `?call_id=` 裡——引擎用它把套接字
釘到應答那個 offer 的憑證上。query 被原樣轉發，正是為了這個。

realtime 會話**絕不被轉換**：只有同方言直通，因為一個能在回應中途繼續說話的用戶端，在
半雙工方言裡沒有對應物。

**Realtime 在准入之後升級。** 先認證、再看握手形狀、再准入、再是上游握手；
`101` 最後才寫。因此每一次拒絕都是用戶端讀得到的 HTTP 應答——一個被接受又立刻關閉的套接字
不帶狀態、不帶 body、也不帶 code。

**被拒絕的上游握手原樣轉達。** 廠商的
`429 {"error":{"code":"insufficient_quota"}}` 比這個閘道器能編出來的任何 502 都值錢。

Realtime 套接字持有它的併發租約直到它**關閉**，而不是到 `101` 被寫出為止。跑一小時的會話就佔
一小時的名額。

### Codex 專有介面

Codex 專有方法僅在使用 Codex 渠道的 Provider 掛載點下提供。假設 Provider 名為
`codex`，可呼叫 `POST /codex/v1/memories/trace_summarize`、
`POST /codex/v1/guardian` 和 `POST /codex/v1/guardian-classifier`。
它們使用正常的操作準入、模型路由和結算流程；公共 `/v1` 不提供這三條路徑。
標準 `/v1/moderations` 使用獨立的 `create_moderation` JSON 契約。

帳號服務也可在 `/codex/v1` 下呼叫，包括 `usage/thread_usage/query_v2`（POST）、
`usage/plan_limit_history`，以及每日 Token、積分、工作區、外掛和技能統計（GET）。
原有 `/codex/api/codex/**`、`/codex/backend-api/**`、`/codex/ps/**` 寫法保留。
管理員設定 `x-gproxy-view: credential:<id>` 可讀取指定憑據的上游報表；預設 Caller
和 Pool 檢視中，無法從本地記錄重建的歷史額度和廠商積分會明確標為不可用，金額為
`null`，不會借用其他帳號的資料或偽造零用量。

### 有歧義的那幾條路徑

`/v1/models`、`/v1/models/{id}` 和 `/v1/files` 被 OpenAI、Claude 和 Gemini 的 v1 surface
拼成了同一個樣子。方言決定向轉換器要哪一套協議型別，猜錯就是用戶端解析不了的 body。

判別依據是**用戶端自己的認證 header**——`x-goog-api-key` 是 Gemini 的、`anthropic-version`
是 Claude 的——因為那是用戶端主動提供的關於它自己的證據，而不是本閘道器發明的預設值。沒有
證據時第一行勝出，而表的順序讓那一行是 OpenAI。

## 掛載點語法

| 路徑 | 掛載點 | 收窄到 |
| --- | --- | --- |
| `/v1/messages` | 聚合 | 不收窄 |
| `/acme/v1/messages` | namespace `acme` | `acme/` 下的公開名稱 |
| `/openai-prod/v1/messages` | Provider `openai-prod` | 那一個 Provider |

**namespace** 是帶斜槓的公開模型名的第一段。**Provider** 掛載點用的是 Provider 的 `name`
——運維者取的標籤，而不是行 id，因為沒有人會把機器生成的 id 敲進用戶端的 base URL。同名時
namespace 勝過 Provider。

**只有在剝掉字首後剩下的部分也是一個被宣告的 surface 時，字首才會被剝掉。** 這條規則的
每一種簡化都是錯的：一個叫 `backend-api` 的 Provider 否則會吃掉
`/backend-api/codex/responses`——那是一條真實的 Codex 路徑——把一次資料面呼叫變成一個並不
存在的掛載點。

**因此有歧義的路徑一律倒向聚合掛載點。** 當第一段確實點名了什麼、但剩下的部分不是本閘道器
提供的 surface 時，路徑原樣保留。丟掉一個掛載點是運維者看得見的 404；錯認一個掛載點則是
把請求發給了錯誤的上游。

掛載點透過**給模型名加字首**來收窄，這是解析器自己的語法而不是第二條規則。已經帶了字首的
名字原樣保留。

:::note[一個已知的缺口]
**不帶模型**的操作在 Provider 掛載點上被收窄到那個 Provider 的*渠道*，而不是那個
Provider。`GET /p1/v1/models` 會列出 `p1` 所在渠道上、呼叫方能觸達的每個 Provider 的模型。
補上它需要請求形狀裡還沒有的一個欄位。
:::

## OAuth issuer

相對某個掛載點字首（`""`、`/acme`、`/openai-prod`）：

```text
GET  {prefix}/v1/oauth/authorize    同意页交接（浏览器则 302 到 `/console/authorize`）
POST {prefix}/v1/oauth/authorize    用户的决定
POST {prefix}/v1/oauth/token        code、refresh 与设备授权
POST {prefix}/v1/oauth/device/code
POST {prefix}/v1/oauth/revoke
GET  {prefix}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{prefix}/v1   （RFC 8414 §3.1）
```

issuer 標識是 `{origin}{prefix}/v1`，這也是聚合掛載點是 `https://host/v1` 而不是
`https://host` 的原因：這樣一條規則就產出全部三個掛載點。RFC 8414 要求這個標識恰好是
用戶端取到文件的那一個，而只有宿主知道請求是從哪個掛載點進來的，所以它是被傳進去的而
不是算出來的。

v3 把它們放在根路徑。移到 `/v1` 之下讓字首規則與 `/v1/messages` 同構，而這次搬遷很便宜
——v3 根本沒實現發現端點，用戶端本來就在硬編碼地址。

## 管理路由

每個身份家族都有同樣的五條路由，由一份宣告生成，因此一個家族不可能不小心只有四條：

```text
GET    /admin/api/{family}           列表（分页与过滤在 query 里）
POST   /admin/api/{family}           创建
GET    /admin/api/{family}/{id}      读取
PATCH  /admin/api/{family}/{id}      更新
DELETE /admin/api/{family}/{id}      删除    → 204
```

**身份**家族：`users`、`api-keys`、`organizations`、`teams`、`permissions`、
`rate-limits`。組織和團隊成員透過各自的成員介面管理。
`oauth-clients` 有前四條，外加 `POST …/{id}/retire` 取代刪除——一個用戶端簽發過的授權還
指著它。

**配置**家族在同樣的五條之上**多一個批次**，因為它們每一個都接受批次，而一次批次無論
點名多少行都是一個 revision 提交：

```text
POST /admin/api/{family}/batch
     [{"create": …}, {"update": {"id": …, "patch": …}}, {"delete": "id"}]
```

`providers`、`credentials`、`models`、`provider-models`、`routes`、`route-members`、
`connection-profiles`、`rule-sets`、`rules`、`provider-rule-sets`、
`operation-rules`、`operation-endpoints`、`quotas`、`price-rules`、`price-rates`、
`price-tiers`。

五條之外還有：`settings`、憑證操作（`reveal`、`status`、`refresh`、`quota`、
`quota-probe`、`quota-reset`、`health-reset`、`limits`）、
`models/{discover,discover/apply,test}`、`rule-sets/{id}/rules`、
`rule-sets/{id}/rule-presets/{preset}`、`providers/{id}/routing-defaults/reset`、
`quotas/status`、`quotas/{id}/{reset,limit-reset}`、`export`、`import`、
`connectivity/test`、`channels`、`tls-presets`、`rule-presets`、
`default-model-catalog`、`tokenizer-vocabs`、`tokenizer-auth`、`session`、`sessions`
和 `audit`。

v4 的管理請求與資料結構已有變化，舊指令碼應按本頁介面重新核對。實例級用量和日誌介面見[用量、日誌與審計](/zh-tw/guides/observability/)。

中介軟體依次是：認證 → **檢查管理範圍與操作權限** → 對不安全的 cookie 請求做同源校驗 → 操作 →
為每個不是讀的方法寫一行審計。它是一個 route layer，因此未知的 `/admin/api/*` 路徑是一個
根本不碰資料庫的 404。

## 使用者面路由

```text
POST   /portal/api/login             用户名 + 密码 → 会话 cookie 与令牌
POST   /portal/api/logout
GET    /portal/api/context           调用方、他们的成员关系、他们的特性开关
GET    /portal/api/models            全部公开名称，各自标注 `permitted`
GET    /portal/api/usage             他们自己的花费
GET    /portal/api/quota             他们自己的预算窗口
GET    /portal/api/requests          他们的近期请求（若已启用）
GET    /portal/api/sessions
GET    /portal/api/keys              （+ POST、DELETE …/{id}、POST …/{id}/rotate、GET …/{id}/secret）
GET    /portal/api/oauth-sessions    （+ DELETE …/{id}）
POST   /portal/api/password
GET    /portal/api/oauth/device      查询待批准的设备码（?userCode=），供 `/console/device` 使用
POST   /portal/api/oauth/device      批准或拒绝；需要登录会话或带管理权限的 key
```

這裡的守衛要求一個已認證的呼叫方，**僅此而已**。沒有角色檢查，因為沒有東西可檢查：使用者面
上的每個操作按構造就限定在構造它的那個呼叫方上，而且沒有一個接收使用者 id。檢查可能被忘掉，
不存在的引數不會。

`login` 和 `logout` 在守衛之外——前者跑在有呼叫方之前，後者必須對一個已經過期的會話也有效，
否則瀏覽器會攥著一個永遠丟不掉的 cookie。

## 解析與失敗轉移預算

四種名字形式與排序見[模型與路由](/zh-tw/guides/models/)。預算：

- 它是路由自己的 `maxAttempts`，被 `settings.maxAttempts` 鉗住；
- 它**跨目標共享**：每個目標最多拿到自己憑證數那麼多次、且不超過剩餘額度，因此一個計劃的
  上游呼叫次數不會超過它的預算；
- 只有"另一個 Provider 可能救得回來"的失敗才換目標——沒有可用憑證、憑證已死、續接釘在別的
  實例上、任何渠道或傳輸錯誤，或 401／403／429／5xx 應答；
- 預算耗盡、被禁止、被取消、轉換錯誤，以及 store 或 cache 故障，都就地停止；
- 沒有目標可換時，**最後一個應答原樣返回**。最後一個 Provider 的 429 就是呼叫方的 429，
  不會被換成別的錯誤。

請求體緩衝一次以便重放。超過 `maxRequestBodyBytes` 的串流體保持串流，計劃隨之裁剪為單個
目標：大檔案上傳不值得為了失敗轉移而全部讀進記憶體。
