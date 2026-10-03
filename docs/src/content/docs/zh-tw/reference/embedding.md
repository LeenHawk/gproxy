---
title: "嵌入核心庫"
description: "gproxy-sdk 是可嵌入的控制代碼：裝配、管理寫入、登入、解析、呼叫、查詢與同步——裡面沒有 server，也沒有身份。"
---

`gproxy-sdk` 是可嵌入的 GPROXY 控制代碼。閘道器本身就建在它上面，而一個應用可以直接連結它。

**它賣的不是 HTTP 用戶端，而是一次帶池化憑證紀律的呼叫**：憑證池、自動重新整理、Provider 之間
的失敗轉移，以及被計量的結算。這也是為什麼資料庫在它內部不能省掉——Claude 每次重新整理都輪換
refresh token，一個把憑證放記憶體裡的控制代碼會在第一次重新整理就把它弄死。

透過 Git tag 或路徑依賴嵌入 `gproxy-sdk`。MIT 協議的配套庫也會單獨釋出到 crates.io。

```toml
[dependencies]
gproxy-sdk = { git = "https://github.com/LeenHawk/gproxy", tag = "v4.0.0" }
```

workspace 使用 Rust edition 2024。本示例固定使用 `4.0.0`；v4 的 Rust API 和管理 API 與 v3 不同。

## 裝配一個

```rust
use gproxy_sdk::{GproxyBuilder, SyncMode};

let gproxy = GproxyBuilder::sqlite("gproxy.db")
    .await?
    .master_key([0u8; 32])
    .sync_mode(SyncMode::Background)
    .build()
    .await?;

for channel in gproxy.channels() {
    println!("{} ({})", channel.display_name, channel.id);
}
println!("serving revision {}", gproxy.revision().0);
```

`build()` 不會開啟任何沒交給它的東西。它同步實體表結構（除非被告知不要）、建立 settings
行、裝配引擎、載入第一份快照，並在背景模式下啟動訂閱與輪詢迴圈。

**金鑰編解碼器絕不隱式選擇。** `master_key` 用 AES-256-GCM 密封，`plaintext_secrets` 是
顯式的反向選擇，兩者都沒有的 build 會被拒絕。不存在一個安靜的預設值，事後才發現它一直是
明文。

## Feature

| Feature | 作用 |
| --- | --- |
| 某個渠道的名字（`codex`、`kiro`、`openai`…） | 把那個渠道編譯進來並註冊 |
| `channels` | 本 workspace 提供的每個渠道 |
| `postgres` / `mysql` | 額外驅動，僅原生。SQLite 在原生上始終可用 |
| `libsql` | 經 Hrana HTTP pipeline 的 libSQL/Turso，**每個目標** |
| `d1` | Cloudflare D1 binding 的標記 |
| `memory`（預設） | 程序內 cache，原生上的預設 |
| `redis` | 多實例用 |
| `fs`（預設） | 本地檔案儲存，僅原生 |
| `s3` | S3/R2 儲存 |
| `bundled-vocabulary`（預設） | 隨包的 DeepSeek 詞表，用於 token 估算 |
| `ts` | 為每個 DTO 和每個渠道 descriptor 生成 `ts-rs` 宣告 |

預設集合能為 `wasm32-unknown-unknown` 構建，`libsql` 也能。

原生構建使用 `reqwest` 傳輸。想要別的後端的宿主——`wreq` 做 TLS 模擬、原生棧匹配 Codex
CLI 的指紋——在 client crate 上開啟那個 feature，或者把自己的 client 池交給 builder。

## 呼叫

```rust
let execution = gproxy
    .call(
        OperationKey { operation: Operation::GenerateContent, dialect: Dialect::OpenAi },
        request,
    )
    .scope("user:u-1")
    .attribution(UsageAttribution { user_id: Some("u-1".into()), ..Default::default() })
    .budgets(vec![BudgetOwner::new("user", "u-1")])
    .credentials(["cred-1".to_owned()].into())
    .send()
    .await?;

let (response, usage) = execution.into_parts();
```

`call` 組裝請求；`send` 解析模型名並走完計劃。

**scope 必填，而且它沒有安全的預設值。** 它是引擎把一個呼叫方的憑證親和性關在裡面的隔離
邊界，一個預設值就意味著陌生人共享續接。

`connect` 是同一個 builder 在 WebSocket 握手上的版本。

`.channel("claudecode")` 收窄候選集。那是收窄，不是選 Provider：渠道對呼叫方有實質差別
——`claudecode` 花的是 OAuth 訂閱額度，`claudeapi` 花的是按 token 計費的 API key，配額池和
計費都獨立——而 Provider 只是一行實例外的人不該需要知道的配置。釘了渠道之後控制代碼的活還很多：
在該渠道的多個 Provider 裡選、在一個 Provider 的多把憑證裡選，並在一把失敗時在渠道內轉移。

應用層決定的一切——允許的 Provider 與憑證、預算鏈、會話——都是**傳進來的**。這裡不認證任何人。

## 這裡沒有什麼

- **身份。** 使用者、API key、組織、團隊、權限、訂閱、限流和 OAuth issuer 屬於 `gproxy-app`，
  它從同一個批裡讀同一個持久 revision。
- **下游認證。** 這裡沒有任何東西決定呼叫方是誰。
- **server。** 沒有監聽器、沒有 router、沒有中介軟體、沒有 CLI。
- **console。** 這個 crate 生成 UI 所依據的 TypeScript 型別；UI 本身是應用的。
- **執行。** 嘗試、轉換、改寫、觀測、預算與結算屬於引擎；這個 crate 只決定交給它什麼。

## 管理

`gproxy.manage()` 是寫側：每個配置家族一個訪問器，全部走同一個提交原語。

| 家族 | 表 | CRUD 之外 |
| --- | --- | --- |
| `providers()` | `providers` | `reset_routing_defaults` |
| `credentials()` | `credentials` | `reveal_secret`、`set_status`、`refresh`、`quota_probe`、`quota_read`、`quota_observations`、`quota_reset`、`health_reset`、`limit_status` |
| `models()` / `provider_models()` | 目錄 | |
| `routes()` / `route_members()` | 路由 | |
| `connection_profiles()` | 出站棧 | |
| `settings()` | 那一行 settings | 只有 `get` / `update` |
| `rewrite()` | 規則集、規則、掛載 | `replace_rules` |
| `endpoints()` | `operation_rules`、`operation_endpoints` | |
| `quotas()` | `quotas` | `budget_status`、`reset_budget`、`limit_status`、`reset_limit` |
| `pricing()` | 規則、費率、檔位 | |
| `transfer()` | 全部配置 | `export`、`import` |
| `catalog()` | 應用前什麼都不寫 | `channels`、`default_models`、`apply_default_prices`、`tls_presets`、`rule_presets` |
| `connectivity()` | `provider_models` | `test`、`model_test`、`discover_models`、`apply_discovered` |
| `tokenizer()` | 詞表檔案 | `vocabularies`、`fetch`、`progress`、`delete`、`auth` |

前十個是普通 CRUD 加一個批次。id 給了就用、沒給就生成，時間戳是 Unix 毫秒，小數以字串
傳輸，而憑證的金鑰從不出現在任何 DTO 裡。

一次寫入宣告自己的 scope，這是呼叫方唯一的選擇。憑證狀態 scope 走便宜的過載路徑，並且
**僅**適用於只改金鑰、過期時間與生命週期狀態的寫入——憑證的其餘部分在裝配時就被凍進執行
快照，改了就必須整體過載。

## 查詢

`gproxy.query()` 是讀側。這裡不寫任何東西，因此也不動 revision。

| 家族 | 回答什麼 |
| --- | --- |
| `usage()` | `records`、`summary`、`group`、`trend` |
| `quota()` | `windows`、`settlements`、`credential_cycles`、`credential_observations`、`counted_windows`、`budget_status` |
| `logs()` | `list`、`detail(request_id)` |

**聚合在 Rust 側完成，並帶掃描上限。** 固定用量欄位使用專有列，每個物理上游呼叫只儲存一條用量，不儲存下游彙總行。供應商、憑證篩選在 SQL 中完成；`summary`、`group` 和 `trend` 對匹配行及
自定義指標按鍵序分塊聚合。每個聚合都有一個掃描預算，預設並被鉗制為 50 000 行，而用滿預算的聚合會帶著
`truncated: true` 和掃描計數回來——絕不把一個更小的數字當成全部事實。趨勢還有第二重邊界：
零或負的桶寬、倒著的區間，以及會產生超過 5 000 個桶的區間，一律拒絕。

費用讀取專有列，統一為 USD；沒有已計價記錄時 currency 為 `None`。
`quantities` 用十進位制字串返回媒體、工具和自定義數量。

**管理列表用 offset 分頁，請求日誌用遊標。** 這不是風格選擇：管理列表是人翻的有界集合，
而請求日誌是一邊被讀一邊在增長的追加流，offset 在它上面會重複或漏行。日誌遊標是兩半的
——一個時間戳加行 id——因為兩個請求可能起始於同一毫秒，只有時間戳的遊標要麼在那一對上
永遠打轉，要麼直接跳過它。

**`logs()` 裡什麼都不脫敏。** 觀察者在*寫*這些行時就應用了部署的策略，所以存下來的已經是
可以展示的。宿主不能假設讀的時候還有第二趟：如果金鑰在資料庫裡，就是策略允許它在那裡，
而讀時過濾撤銷不了這件事。

## 同步

| | 傳遞什麼 | 會怎麼失敗 |
| --- | --- | --- |
| 共享 cache 上的失效通知 | 毫秒級的"再看一眼" | 丟訊息 |
| `settings.config_revision` 輪詢 | 持久事實，預設 30 秒 | 慢 |

wasm 上 cache 預設是資料庫承載的那一個，而同步一律是**手動**的：isolate 活不過一次請求，
沒有背景迴圈可跑，所以請求開頭的 `tick()` 就是全部機制。

```rust
// tick() = 取走已投递的通知，然后轮询一次 revision。
instance.tick().await;
```

取通知便宜到可以每請求都做，輪詢不是，因此只想要便宜那一半的宿主可以單獨調它。手動模式
在 build 時就建立訂閱並丟掉隊首的 resync 提示——首次裝載*就是*那次 resync——否則介於裝配
與第一次 tick 之間的寫入會被兩套機制同時漏掉。

## TypeScript 匯出

```sh
GPROXY_TS_OUT=console/src/generated \
  cargo test -p gproxy-sdk --features ts export_types
```

沒有這個環境變數時測試直接返回、什麼也不寫，所以 `cargo test --all-features` 保持無副作用，
而生成目錄只會被有意地重寫。有它時，目錄先被**清空**——一個已經不存在的 DTO 留下的陳舊
宣告，會在 Rust 早就刪掉它之後還讓型別檢查透過——最後寫一個把一切 re-export 出去的索引。

渠道目錄也在裡面。console 從渠道自己返回的 descriptor 渲染 Provider 表單，所以它應該被
那份 descriptor 定型，而不是被一份會在渠道下次新增配置鍵時漂走的副本。

匯出清單是手寫的，因為 Rust 沒有執行期列舉模組型別的辦法。那份清單正是 v3 漂走的東西
——加了一個 DTO，沒人記得加進清單，於是 console 悄悄地沒有了它的型別——所以第二個測試讀
模組自己的原始碼，把它的 re-export 與清單對比。**給 dto 加一個型別卻忘了加進清單，是一個
紅色的測試，而不是一個缺失的檔案。**

## SDK 之上

同時想要身份的應用連結 `gproxy-app`，它補上認證、准入、管理面與使用者面的操作家族以及
OAuth issuer，而且**不含**任何傳輸層。`gproxy-host-axum` 把它們變成一個 HTTP surface，
`gproxy-host-edge` 在 Worker 裡掛載同一個 router，`gproxy-host-tauri` 把管理面綁到 IPC。

其中每一道接縫都是嵌入者可以進入的地方。見[架構](/zh-tw/introduction/architecture/)。
