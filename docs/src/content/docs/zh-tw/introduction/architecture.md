---
title: 架構
description: GPROXY v4 的構造——不含 server 的引擎、不含傳輸層的產品層、三個宿主共用一張路由表，以及讓它們彼此分開的接縫。
---

v4 是對 v3 的徹底重寫。要做的事一樣，結構不一樣。本頁是地圖：crate、它們之間的接縫，
以及不讓結構漂回去的那幾條規則。

能解釋其餘一切的一句話：**每一層只回答一類問題，向下交出一個收窄後的答案，下層絕不
重新開啟它。**

## 兩種裝配

```text
sdk  =  引擎 + 配置写入 + 登录 + 解析 + 查询 + 同步
app  =  sdk + 身份与准入 + 管理面／用户面／issuer 的类型化操作
host =  app + 一个传输层
```

`gproxy-sdk` 是可嵌入的控制代碼。它用預設實現裝配出引擎，擁有推進
`settings.config_revision` 的配置寫入，把一次登入變成一行憑證，把模型名解析成執行計劃，
並讓部署裡的每個實例停在同一個 revision 上。它**不含 HTTP server，也不含身份**。

`gproxy-app` 補上"誰在呼叫、能觸達什麼"：使用者、閘道器 API key、組織、團隊、權限、訂閱、
限流、OAuth issuer、審計，以及寫入其中任何一項的操作。它**不含 server**——沒有 router、
沒有 runtime、沒有 CLI——這正是同一套判斷既能跑在原生 axum server 後面、也能跑在
Cloudflare Worker 裡的原因。

宿主是剩下那層薄殼：位元組進來，型別化呼叫出去，位元組回去。

## crate 清單

| Crate | 裝什麼 | 絕不 |
| --- | --- | --- |
| `gproxy-protocol` | 連線建模（`WireRequest`、`WireResponse`、body、分幀、套接字）、操作登錄檔、OpenAI／Claude／Gemini 協議型別，以及協議適配所需的宿主能力介面 | 知道渠道存在；匹配 URL 路徑 |
| `gproxy-channel` | `BaseChannel`、可選能力 trait，以及按 feature 啟用的上游介面卡 | 依賴引擎或 store；選擇 client 後端 |
| `gproxy-client` | 出站傳輸契約及其 `reqwest` / `wreq` 實現，加上 wasm 的 `fetch` 與 Workers 後端 | 讀資料庫、選憑證、做協議轉換 |
| `gproxy-core` | 引擎：Provider 執行快照、允許集合內的憑證選擇／重新整理／健康、轉換、改寫、預算、計價、結算與觀測 | 解析路由、檢查權限、跑 server |
| `gproxy-store` | 表結構與查詢，後端按 feature 切 | — |
| `gproxy-cache` | TTL 狀態、原子計數／許可／租約、跨實例失效通知；Memory 與 Redis | 業務實體；後端故障時靜默降級 |
| `gproxy-seaorm` | SeaORM 批次讀寫、D1 binding、型別對映、結構同步 | 業務實體 |
| `gproxy-tokenizer` | 字串 → token 數，基於本地共享詞表 | 解析請求、下載、背景任務 |
| `gproxy-file` | 可選的本地或 S3/R2 內容儲存，基於 OpenDAL | 檔案後設資料；歸屬關係 |
| `gproxy-sdk` | 控制代碼：裝配、`manage()`、`login()`、解析、會話、`call()`、`query()`、`sync` | 任何 HTTP server 程式碼；任何身份 |
| `gproxy-app` | 身份、准入、兩個產品面和 issuer | 傳輸層 |
| `gproxy-host-axum` | HTTP 路徑 → 操作。那張路由表 | 自己的產品判斷 |
| `gproxy-host-edge` | `fetch` → 那個 axum router | 第二張路由表 |
| `gproxy-host-tauri` | IPC → 操作，只綁管理面與使用者面 | 資料面 |
| `gproxy` | 命令列：環境、檔案、socket、終端 | 任何庫能做的事 |

箭頭只朝一個方向：宿主依賴 app，app 依賴 sdk，sdk 依賴引擎。引擎永遠不會長出監聽器、
router 或 UI。

## 每個問題在哪裡被回答

| 問題 | 誰回答 |
| --- | --- |
| 誰在呼叫 | `gproxy-app` 的認證器 |
| 能觸達什麼、花誰的錢 | `gproxy-app` 的准入 |
| 哪個 Provider 承接這個模型名 | sdk 的解析器 |
| 哪把憑證、失敗了怎麼辦 | 引擎 |
| 一個操作*做*什麼 | `gproxy-app` 的操作家族與 sdk 的 `manage()` |
| 一次失敗線上上值什麼狀態碼 | `AppError::status_code` / `code` |
| 哪個 URL 對應哪個操作 | axum 宿主 |

任何宿主若重新回答前五個中的任何一個，它的答案只會應用在 HTTP 上而不會應用在 IPC 上，
而這正是接縫存在要防的那一類 bug。

## 解析與 Plan

解析發生在**進引擎之前**，所以引擎只見已解析的目標。模型名按第一條命中的規則解析：

| 名字 | 解析為 | 嘗試預算 |
| --- | --- | --- |
| 沒有模型 | 全部啟用的 Provider，無上游模型 | `settings.max_attempts` |
| 公開模型名 | 該路由啟用的成員 | 路由自己的 |
| `渠道/模型` | 該渠道的 Provider，優先目錄裡列了該模型的 | `settings.max_attempts` |
| `Provider 名/模型` | 那一個 Provider | `settings.max_attempts` |
| 其他 | unknown model 錯誤 | — |

公開名精確匹配，並且**先於**字首形式，因此運維者可以把字面量 `openai/gpt-5` 作為自己的
公開名暴露出來。

字首形式內部，**渠道 id 勝過同名 Provider**。渠道 id 由構建固定、改不掉；Provider 名隨時
可以改。反過來更糟：有人把 Provider 命名為 `codex`，全部 `codex/*` 流量就再也到不了
`codex` 渠道，而且沒有任何繞開的辦法。

候選隨後按渠道、允許的 Provider id、允許的憑證 id 收窄——最後一項是產品層做組織間隔離的
手段。排序是 `(tier, 健康度, 权重倒序, 稳定 id)`；只有首段按路由策略做均衡。

## 失敗轉移有兩個維度

引擎已經在單個 Provider 的憑證之間重試過了。sdk 是另一個維度，**只有"另一個 Provider
可能救得回來"的失敗才換目標**：沒有可用憑證、憑證已死、續接釘在別的實例上、任何渠道或
傳輸錯誤，以及 401／403／429／5xx 應答。預算耗盡、被禁止、被取消、轉換錯誤，以及 store
或 cache 故障，都就地停止——在別處重試只是多花一點。

嘗試預算跨目標共享，因此一個計劃的上游呼叫次數不會超過它的 `max_attempts`。沒有目標可換
時，最後一個應答原樣返回——最後一個 Provider 的 429 就是呼叫方的 429，不會被換成別的錯誤。

## 一次寫入 = 一個 revision + 一次過載 + 一次通知

```text
commit_revision([语句…, config_revision += 1, 读回])   一个事务
      ↓
reload   本实例的快照
      ↓
publish  ConfigurationChanged { revision, scopes }
```

行與 revision 自增是同一個事務。拆開就有兩個都會真實發生的視窗：行落庫而自增沒落，
同伴永遠不知道該過載，這條配置在別的實例上隱身；或者自增落庫而行沒落，所有同伴為一份
不存在的變更各過載一次。

**過載必須在通知之前。** 反過來就等於宣告一個自己還服務不了的 revision。通知本身是盡力
而為——cache 拒絕釋出只讓部署多等一個輪詢週期，這正是輪詢兜底存在的理由。

兩套機制，缺一不可：

| | 傳遞什麼 | 會怎麼失敗 |
| --- | --- | --- |
| 共享 cache 上的失效通知 | 毫秒級的"再看一眼" | 丟訊息 |
| `settings.config_revision` 輪詢 | 持久事實，預設 30 秒 | 慢 |

過載序列且單調推進：舊 revision 絕不覆蓋新的，過載失敗保留上一份快照繼續服務。

## 結算不是可選項

引擎保證每一條到達上游的路徑都走同一個漏斗：給交換定價、扣減每一個適用的預算視窗、
把報告交給觀察者。沒有繞過它的快路徑，因為繞過它的路徑就是未計量流量。

*可選*的東西是**幹活之前先問**，而不是事後丟棄結果。關掉的 capture 不分配任何東西、
不復制任何 body；v3 的做法是先把整份成本付掉，再讓 sink 決定要不要。

費用在上游呼叫結束後結算，在途請求和併發呼叫可能使預算超出上限。結算按請求 id 冪等，結算失敗只丟記賬，不影響已經交付的回應。

沒有價格規則覆蓋的模型按 0 結算，並帶上 `unpriced = true` 維度。運維者要的是這個訊號，
不是一個拒絕。

## wasm 是目標，不是分叉

`gproxy-app` 能為 `wasm32-unknown-unknown` 構建，axum router 也能，這正是
`gproxy-host-edge` 掛載同一個 `Router` 而不是把路由表寫第二遍的原因。axum 的 server
那幾半是 Cargo feature 而不是 crate 本身；wasm 構建少掉的東西都是 socket 形狀的，從來
不是一條路由。

真正的障礙是 `Send`。wasm 上引擎**按設計**是 `!Send`——JS 傳輸控制代碼屬於建立它的 isolate
——而 axum 要求 state `Send + Sync`、handler future `Send`。兩處執行期檢查的橋接解決了它，
都成立於"Worker isolate 是單執行緒"：app 把控制代碼放進 `SendWrapper`，axum 宿主包住每個
handler 體。漏包的 handler 是 **wasm 目標上一個指名道姓的編譯錯誤**——這就是強制手段，
也是它放在 handler 而不是 router 的 `cfg` 上的原因。**編不過 edge 的路由，不可能在 edge
上悄悄不存在。**

## 不讓它漂回去的規則

- 引擎永遠不依賴 server 框架、router 或 UI。
- 每一條到達上游的請求都從同一個漏斗出去。
- 轉換是成對的；沒有中間表示，轉換器也不讀寫未知欄位袋。
- 一次配置寫入就是一個事務加它的 revision 自增。
- 快照發布是單調的；過載絕不倒退。
- 裝配絕不因為一行壞資料而失敗——丟掉、計數、記日誌。
- edge 宿主沒有自己的路由表。
- 前端型別由 Rust 生成，絕不手寫。
