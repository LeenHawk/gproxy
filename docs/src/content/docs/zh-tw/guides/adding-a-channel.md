---
title: 新增渠道
description: "一個 v4 渠道的結構：BaseChannel 契約、可選能力 trait、從一個 Cargo feature 到註冊完成的步驟，以及如何在本倉庫之外釋出一個渠道。"
---

一個渠道只知道一族上游：它的 URL、憑證怎麼注入、它講哪些線方言、它的流怎麼報 usage，
以及——對帳號型上游——怎麼登入、重新整理和讀配額。

其餘都是別人的事。路由、憑證選擇、失敗轉移、協議轉換、結算和 capture 屬於引擎，而且
**渠道里沒有任何東西讀資料庫或挑傳輸後端**。

渠道是編譯進去的，從不在執行時載入，它可以放在兩個地方：

- **放在本倉庫裡**：`gproxy-channel` 的一個模組，藏在它自己的 Cargo feature 後面，透過 PR
  加入。它能到達每一個宿主，包括 Workers 宿主。見[九步](#九步)。
- **放在你自己的 crate 裡**：一個對著公開契約實現 `BaseChannel` 的型別，加上一個"原版
  + 這個型別"的原生二進位制或 Worker。本倉庫裡什麼都不用改。見[在本倉庫之外](#在本倉庫之外)。

兩種方式實現的是同一個 trait，寫法也一樣；不同的只是註冊方式。預設不編譯任何具體渠道。

## 它需要一個渠道嗎

渠道是要對著別人的 wire 維護的程式碼。只有當一行供應商說不清它需要什麼時——路徑隨操作
移動、body 必須被改寫、有帳號 surface 要讀、有用戶端身份要呈現——一個廠商才值得一個渠道。

全部差別只是一個源和一個 header 的廠商，就是一個 `custom` 供應商。見
[不需要渠道的廠商](/zh-tw/guides/providers/#接入相容服務)。

## 契約

`BaseChannel` 是唯一必需的 trait，`id` 是它唯一必需的方法。每個協議操作都有自己的非同步方法
並帶**預設實現**：用 `prepare`（HTTP）或 `prepare_connect`（WebSocket）構造請求，再把它交給
被指派的 client。

兩個準備鉤子的預設實現都是"不支援"，因此**一個渠道恰好支援它準備或覆寫的那些，絕不更多**。

```text
宿主选定供應商、凭证、client
  → ChannelBinding::new(&channel, provider, credential, client)
  → binding.send(OperationKey, WireRequest<HttpBody>)
    binding.connect(OperationKey, WireRequest<()>)
  → BaseChannel 上那个具名操作方法
  → 默认：prepare + client.send  /  prepare_connect + client.connect
    或渠道自己经同一个 client 的多次调用流程
```

宿主交過來的檢視是借用的，能公開的都公開：

| 型別 | 裝什麼 |
| --- | --- |
| `ProviderView` | `id`、`channel`、可選 `base_url`，以及渠道自己解碼成型別化設定的 `config` JSON |
| `CredentialView` | `id`、`provider_id`、`auth_kind`、`secret` JSON（無 `Debug`、無 `Serialize`）、宿主記錄的公開 `metadata`、`version`、`expires_at_ms` |
| `OperationContext` | 兩個檢視、操作的方言、`WireRequest`、一個持有的 client、跨請求狀態、宿主 `instance_id` 和可選的端點覆蓋 |
| `CredentialContext` | 重新整理、配額與服務用的供應商、憑證和 client |

### 每個渠道都遵守的規則

- **準備是同步且純的**：不做 I/O、不藏狀態、不構造 client。操作覆寫可以做多次交換，但只能
  經 `context.client`，並且可以在回應流結束後繼續持有它完成收尾工作。
- **來源鑑權絕不到達上游。** 轉發助手丟棄逐跳 header，外加 `host`、`content-length`、
  `authorization`、`x-api-key`、`x-goog-api-key` 和 `api-key`；渠道從憑證里加上自己的
  鑑權。全域性白名單、供應商的 `allowed_headers` 與渠道宣告的頭取並集，
  `content-type` 總是保留。未配置或空列表不增加允許項。
- **端點覆蓋是完整的方法 URL**，替換 base URL 加渠道預設路徑——不是一個拿來再拼的 base。
- **非 2xx 是一個回應，不是一個錯誤。** 狀態、header 和一個惰性 body 原樣回來。錯誤變體
  是給*能力*呼叫（登入、重新整理、配額）和多次呼叫覆寫裡的中間交換用的，那裡失敗的應答不是
  要返回的那個回應。
- **`RefreshRejected` 意味著上游確定性地拒絕了這把憑證**（`invalid_grant`、已撤銷），
  宿主會把它標記為死。短暫的傳輸失敗或 5xx 絕不能：它必須以傳輸錯誤浮現，好讓宿主稍後
  重試。這一條弄錯就會殺掉一個正常的帳號。
- **`ContinuationElsewhere`** 表示一條活的上游連線被另一個宿主程序持有。宿主會改道；
  憑證本身沒有任何問題。

## 可選能力

每一項都是一個獨立 trait，藏在預設返回 `None` 的訪問器後面。渠道實現它的上游擁有的那些，
而且彼此之間沒有任何強制耦合——一個渠道可以報告配額視窗卻不提供重置。

| 訪問器 | 用途 |
| --- | --- |
| `credential_refresh` | 產出一份**完整替換**的金鑰與過期時間；宿主以 `version` 上的 CAS 寫回 |
| `oauth_authorization_code` | 用宿主提供的 PKCE challenge 與 state 構造授權 URL；把 code 換成憑證 |
| `oauth_device_code` | `start` 與一步 `poll`。節奏是宿主的事 |
| `cookie_login` | 把貼上來的 cookie 換成一份憑證加它的公開後設資料 |
| `quota_model` | 同步且純：從 `auth_kind` 與 metadata 推出這把憑證有哪些配額維度 |
| `quota_query` | 從上游的用量端點讀一份配額快照 |
| `quota_headers` | 把回應 header 變成配額條目；空表示什麼都沒報 |
| `quota_reset` | 在賣重置額度的上游上兌換與手動重置 |
| `usage_extras` | 廠商自己的用量欄位，在宿主已經從整形後的回應裡讀到的標準用量之外補充 |
| `response_reason_observer` | 給原始上游回應分類（限流、過載、拒絕……）。預設實現解析標準 JSON 與 SSE；只有私有 wire 格式才需要覆寫 |
| `websocket_response_reason` | 同上，針對一條完整的入站 WebSocket 訊息 |
| `services` | 廠商控制面路由——見 [CLI 用戶端](/zh-tw/guides/cli-clients/) |

用量本身不需要任何能力：宿主從渠道整形出的標準回應裡讀取它，一個什麼都沒報的渠道是
**未報告**，不是 0。

有兩項 descriptor 能力無法從訪問器推出，具備它們的渠道要在自己的 `descriptor` 裡寫明：
`websocket`（它覆寫了某個 socket 操作）和 `paid_usage`（它的
`QuotaModel::allows_paid_usage` 讀取憑證 metadata 標誌 `allow_paid_usage`，控制檯正是據此
顯示那個開關）。

跨請求記憶由宿主限定在一個供應商加一把憑證的範圍內，並以 CAS 寫入。binding 的預設實現
拒絕一切寫入，因此想要狀態的渠道必須被明確授予。

### 登入口袋

兩種 OAuth 流程都帶著同一個口袋，用來存渠道在使用者完成授權之後需要的事實：`start` 放進去
的東西會隨授權一起回來。它的形狀是渠道自己的——一個非標準的裝置控制代碼、一個登入過程為自己
註冊的 client。

**這個口袋可以帶金鑰**，與宿主會當作憑證 metadata 公開的 `provider_fields` 不同：它活在
宿主的登入會話裡，短命、由 cache 承載、從不被渲染，登入結束就沒了。任何必須活過登入的
東西都要從 exchange 返回——公開事實進 metadata，秘密進供應商 secrets，後者與令牌一起
密封，永遠不會變成 metadata。

## 九步

拿 `custom`（API key）和 `codex`（OAuth 帳號）當兩個參考。

1. **在 `crates/gproxy-channel/Cargo.toml` 裡宣告 feature**，只列這個渠道需要的可選依賴。
   僅 wasm 需要的依賴放進 `cfg(target_arch = "wasm32")` 的 target 表。

   ```toml
   [features]
   # 一行說明上游是什麼、這個渠道覆蓋到哪。
   acme = ["dep:base64", "dep:web-time"]
   ```

2. **往 `src/channels/mod.rs` 的 `channels!` 列表里加一行**：feature、模組，以及宿主要註冊
   的那個值。這一行同時宣告模組並把渠道放進 `compiled_in()`，而那正是每個宿主找到它的方式。

   ```rust
   "acme" => acme, acme::Acme;
   ```

   如果這個渠道用到 `shared`，也要擴充套件那個模組自己的 `cfg(any(...))`。

3. **一個檔案一件事地鋪開模組。** 小渠道就是一個 `acme.rs`；大的是一個目錄：

   ```text
   src/channels/acme/
     mod.rs        id、單元結構體、re-export、模組文件
     config.rs     供應商的 `config` JSON，serde(default)，忽略未知鍵
     request.rs    prepare / prepare_connect / 被覆寫的操作
     oauth.rs      登入與重新整理能力
     quota.rs      配額能力
     usage.rs      用量能力
     services.rs   廠商控制面路由
   ```

   **模組文件要點名每一條 wire 事實的來源**——廠商的 API 參考，或 `samples/` 下的 CLI
   原始碼。wire 真相絕不是另一個渠道的程式碼。

4. **實現 `BaseChannel`。** 最少是 `id`、`native_dialects` 和 `prepare`。結構體是無狀態
   單元，所以一個實例服務該渠道的每一個供應商。

   也要覆寫 `descriptor`。預設實現只讀能力訪問器，而只有渠道自己知道它的顯示名，以及它
   從供應商 `config` JSON 裡解碼哪些鍵。**那份 descriptor 是管理 UI 渲染供應商表單
   的全部依據**——在 `ts` feature 下它還是一個 TypeScript 型別——所以漏掉的鍵就是沒人能設的鍵。

   當上遊需要多次交換、需要本地合成的應答、需要另一種傳輸，或需要把回應改寫成宣告的原生
   方言時，覆寫具體的操作方法而不是 `prepare`。

5. **宣告廠商用戶端會發什麼。** 如果渠道模擬某個 CLI，匯出它的 header 常量並用它構造
   allow-list，這樣供應商的 `allowed_headers` 就剝不掉 CLI 自己的 header。把這些身份
   header 傳進轉發助手的丟棄列表，讓**用戶端無法偽造它們**。上游對用戶端做指紋時返回一份
   預設連線配置；憑證或供應商上的顯式配置依然勝出。

6. **把能力做成獨立型別**並從訪問器返回。守住每個 trait 的規則：重新整理返回完整替換而不是
   合併；配額維度從憑證 metadata 讀套餐事實，絕不走網路；用量觀察者做累積快照，而它的
   `finish` 從**宿主**收到"完成還是被打斷"，因為光有 EOF 並不能確立完整的用量。

7. **可複用的 wire 機制放進 `src/channels/shared/`**，按使用它們的 feature 門控。策略留在
   渠道里；shared 模組只執行渠道要求的事。

8. **測試貼著程式碼放**在 `tests/<id>.rs`，用 `#![cfg(feature = "…")]` 門控。手工構造
   供應商與憑證檢視，用一份 fixture 金鑰調 `prepare`，斷言 URL、注入的鑑權和被丟棄的
   header。把抓到的幀餵給用量觀察者；解析錄下來的配額 header 與用量 body。一個指令碼化的
   出站 client 用來演練多次呼叫覆寫。**不要從這裡測引擎。**

9. **把 feature 轉發到每一個宿主。** 引擎從不自己列渠道：`gproxy-sdk` 註冊
   `compiled_in()` 返回的全部渠道，所以一個渠道能否被某個二進位制用到，恰好取決於那個二進位制的
   feature 集合能否到達它的 feature。用同一個名字把 feature 加到：

   | 檔案 | 要加的行 |
   | --- | --- |
   | `crates/gproxy-sdk/Cargo.toml` | `acme = ["gproxy-channel/acme"]`，並把 `"acme"` 加進它的 `channels` 列表 |
   | `crates/gproxy/Cargo.toml` | `acme = ["gproxy-sdk/acme"]` |
   | `crates/gproxy-host-edge/Cargo.toml` | `acme = ["gproxy-sdk/acme"]` |
   | `crates/gproxy-app/src/v3/channels.rs` | 把 id 加進 `V4_CHANNELS`，讓 v3 匯入接受它 |

   重複 id 會被拒絕，而點名了未註冊渠道的供應商是一個配置錯誤，不是一次回退。

## 收尾

```sh
cargo fmt --all
cargo clippy -p gproxy-channel --all-features --all-targets -- -D warnings
cargo clippy -p gproxy-channel --all-features --target wasm32-unknown-unknown --lib -- -D warnings
cargo test   -p gproxy-channel --all-features
```

**每個渠道都要能為原生目標和 `wasm32-unknown-unknown` 構建**，這正是 Workers 宿主不是一個
縮水渠道集的原因。lint 報錯要改程式碼，不是加 `#[allow]`。

進去之後就不需要別的了：新渠道上的供應商會帶著它的 descriptor 出現在
`GET /admin/api/channels` 裡，管理 UI 從那份 descriptor 渲染它的表單。

## 在本倉庫之外

渠道可以放在自己的 crate 裡，不用 fork 就能釋出，兩個宿主都行：一個原生二進位制，它就是原版
`gproxy`——同樣的命令、引數、控制檯和資料庫；或者一個 Worker，它就是原版 GPROXY Worker。
兩者都只是在內建渠道旁邊多註冊了你的渠道。`examples/channel-plugin` 是一套完整、可執行的
三個 crate：一個檔案實現的 OpenAI 相容 API key 渠道，以及每個宿主幾行程式碼的入口。

### 目錄結構

```text
my-gproxy/
  channel/    渠道：一個實現 BaseChannel 的單元結構體，不依賴任何宿主
  native/     gproxy 二進位制，帶上它
  edge/       Cloudflare Worker，帶上它（只在部署到那裡時需要）
```

渠道單獨成一個 crate，兩個宿主才能都依賴它。原生宿主不能是它的依賴：`gproxy` 編不了
wasm，Worker 也就跟著編不了。

```rust
// native/src/main.rs
use std::{process::ExitCode, sync::Arc};

fn main() -> ExitCode {
    gproxy::main_with(vec![Arc::new(my_channel::Acme)])
}
```

`main_with` 就是原版 `main` 的全部：載入 `.env`、解析同樣的引數、建執行時、執行。自己有
執行時的宿主改調 `gproxy::run_with(cli, channels)`。

```rust
// edge/src/lib.rs
#[cfg(target_arch = "wasm32")]
mod entry {
    use std::sync::Arc;

    use worker::{Context, Env, HttpRequest, event};

    #[event(fetch)]
    async fn fetch(
        request: HttpRequest,
        env: Env,
        ctx: Context,
    ) -> worker::Result<gproxy_host_edge::Response> {
        gproxy_host_edge::fetch_with(request, env, ctx, || vec![Arc::new(my_channel::Acme)])
            .await
    }
}
```

`fetch_with` 就是原版 fetch handler 的全部。一個 Worker 模組只能有一個 `#[event(fetch)]`，
所以 edge crate 依賴 `gproxy-host-edge` 時要**關掉**它的 `entry` feature——原版 handler
就是那個 feature 宣告的。閉包在每個 isolate 裡只執行一次，也就是第一個請求組裝例項的時候。

兩個宿主裡，`id` 與某個內建渠道相同的渠道都會**替換**那個內建渠道——要釋出一個改過的現有
渠道，就這麼做。

### 依賴

按路徑依賴一份 gproxy checkout，版本就是你要對著構建的那個：

```toml
# channel/Cargo.toml
[dependencies]
gproxy-channel = { path = "../../gproxy/crates/gproxy-channel" }
gproxy-protocol = { path = "../../gproxy/crates/gproxy-protocol", features = ["exhaustive"] }
http = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```toml
# native/Cargo.toml
[dependencies]
my-channel = { path = "../channel" }
gproxy = { path = "../../gproxy/crates/gproxy", features = ["channels", "memory", "fs", "embedded-console"] }
```

```toml
# edge/Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
my-channel = { path = "../channel" }
gproxy-host-edge = { path = "../../gproxy/crates/gproxy-host-edge", default-features = false, features = ["channels", "d1", "libsql"] }
worker = { version = "0.8", default-features = false, features = ["http"] }
```

```toml
# Cargo.toml，workspace 根
[workspace]
resolver = "3"
members = ["channel", "native", "edge"]

# 從 checkout 根目錄的 Cargo.toml 複製：patch 只在宣告它的 workspace 裡生效，
# 而構建依賴這些。
[patch.crates-io]
sqlx-sqlite = { path = "../gproxy/vendor/sqlx-sqlite" }
btls-sys = { git = "https://github.com/0x676e67/btls", rev = "609f955e6de4df4a215ee9b273d327af1c0771db" }
```

第一次構建前把 checkout 裡的 `Cargo.lock` 複製到根 `Cargo.toml` 旁邊，這樣解析出來的依賴
版本就是 gproxy 測試過的那些。

用 checkout 而不是 git 依賴，只有一個原因：原生控制檯是從
`crates/gproxy-host-axum/assets/web` 編進二進位制的，而那個目錄沒有提交。先在 checkout 的
`console/` 裡跑 `pnpm install && pnpm build`；或者從 features 裡去掉 `embedded-console`，
得到一個不帶控制檯的二進位制。下面的 Worker 構建也從 checkout 取控制檯。

`gproxy` 和 `gproxy-host-edge` 自己的 feature 列表和原版構建一樣用：`channels` 是全部
內建渠道，也可以只點名部署要用的那些。

### 構建 Worker

checkout 裡的 `deploy/cloudflare` 能構建任何一個 Worker crate，不只是原版那個。用絕對路徑
指向你的 crate，然後照常部署：

```sh
cd ../gproxy/deploy/cloudflare
pnpm install
GPROXY_EDGE_CRATE=/absolute/path/to/my-gproxy/edge pnpm run build
pnpm run deploy
```

它會在你的 crate 裡跑 `worker-build`、裁掉除錯段，再把 Worker 複製到 checkout 的
`wrangler.toml`、控制檯資源和它們的 header 旁邊。[Edge 部署指南](/zh-tw/deployment/edge/)
裡的一切——D1 繫結、金鑰、`GPROXY_CONFIG`——原樣適用。

**給 Worker 用的渠道必須能編到 `wasm32-unknown-unknown`**，每個內建渠道都已經遵守這條規則。
實際上就是：不用 `tokio` 網路、不碰檔案系統、用 `web-time` 而不是 `std::time::Instant`，
所有請求都走宿主交過來的 client。用
`cargo check --target wasm32-unknown-unknown -p my-channel` 檢查。

### 公開契約給了你什麼

渠道要實現或會收到的一切都在 `gproxy_channel::channel` 裡公開：`BaseChannel`、上下文與
檢視型別、每一個能力 trait、descriptor 型別，以及構造請求要用的助手——處理用戶端 header 的
`forwardable` 與 `HeaderAllowlist`、寫入運維靜態 header 的 `insert_configured`、清理查詢串的
`strip_query_auth`，還有 `standard_reason_observer`。協議詞彙（`Dialect`、`Operation`、
`OperationKey`、`HttpBody`、`WireRequest`）來自 `gproxy_protocol`。

`gproxy_channel::channels::shared` **不**公開：它是為本倉庫裡的渠道調的，會隨它們一起變。
需要什麼就複製什麼。

上面的規則原樣適用——尤其是準備必須是純的、來源鑑權絕不到達上游。

### 與內建渠道的不同

- **控制檯和對待其他渠道一樣，從 descriptor 渲染你的渠道**：供應商表單來自
  `config_keys`，憑證表單來自 `credential_kinds`，登入按鈕來自 `login_modes`，配額、重置、
  重新整理與付費用量開關來自 `capabilities`。它沒有的是為內建 id 寫的那些逐渠道潤色——量身定製的
  提示文字、模型預設組。
- **原生二進位制的自更新會拒絕安裝。** 釋出的 release 是本倉庫的構建；把它換上來會丟掉你的
  渠道，下次重啟後它上面的每個供應商都會失效。請對著新版本重新構建。檢查更新照常可用。
- **契約跟著 gproxy 的版本走。** 實際上的變化都是增量的——大多數協議型別帶
  `#[non_exhaustive]`，`BaseChannel` 的新方法都帶預設實現——但請對著你執行的那個版本構建。
  寫 `ChannelCapabilities { quota_query: true, ..Default::default() }` 而不是列出每個欄位，
  這樣新增一項能力不會讓你的構建失敗。

## 給編碼 Agent 的清單

同樣的步驟，按 agent 應當執行的方式列出。

**內建**（向本倉庫提 PR）：

1. 以 `crates/gproxy-channel/src/channels/nvidia.rs`（API key）或 `codex/`（OAuth）為模板，
   並閱讀 `samples/` 下廠商的 wire 原始碼或它的 API 參考。
2. 在 `crates/gproxy-channel/Cargo.toml` 里加 feature，在 `src/channels/mod.rs` 的
   `channels!` 里加一行，並把 feature 加進該渠道用到的 `src/channels/shared/` 裡每一個
   `cfg(any(...))`。
3. 寫 `src/channels/<id>.rs` 或 `src/channels/<id>/`；覆寫 `descriptor`。
4. 在第 9 步列出的四處轉發 feature。
5. 如果渠道認 `allow_paid_usage` 或提供 WebSocket，在它 descriptor 的 capabilities 裡設
   `paid_usage` / `websocket`。
6. 在 `docs/src/i18n/reference/zh-cn.json` 和 `zh-tw.json` 中補齊新增 descriptor／Schema 說明的翻譯，
   執行 `pnpm --dir docs generate:reference`。它會匯出已註冊通道，缺少翻譯時會失敗。不要提交產生的 JSON。
7. 跑[收尾](#收尾)下的命令。

**在本倉庫之外**：

1. 把 `examples/channel-plugin` 複製到 checkout 之外——`channel/` 必須要，`native/` 和
   `edge/` 按你要部署的宿主取。加上上面那個帶 `[patch.crates-io]` 表的 workspace 根
   `Cargo.toml`，並複製 `Cargo.lock`。
2. 在每個 crate 的 `Cargo.toml` 裡，用包自己的 `version` 和 `edition = "2024"` 替換
   `*.workspace = true` 這些鍵，把每個 `workspace = true` 依賴改成上面寫的路徑依賴。
3. 重新命名包、二進位制、結構體和 `ID`。除非就是要替換某個內建渠道，否則選一個內建渠道沒用過的 id。
4. 只對著 `gproxy_channel::channel` 實現；不要去用 `channels::shared`。
5. `cargo clippy --all-targets -- -D warnings`；要部署 Worker 的話再跑
   `cargo check --target wasm32-unknown-unknown -p <channel> -p <edge>`。
6. 在一個全新的 `GPROXY_DATA_DIR` 上執行原生二進位制，或者按上面的步驟構建後在
   `deploy/cloudflare` 裡用 `wrangler dev --local` 執行 Worker，確認
   `GET /admin/api/channels` 裡列出了這個 id。
