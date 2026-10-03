---
title: 新增通道
description: "一個 v4 渠道的結構：BaseChannel 契約、可選能力 trait、模組佈局，以及從一個 Cargo feature 到註冊完成的九步。"
---

一個渠道只知道一族上游：它的 URL、憑證怎麼注入、它講哪些線方言、它的流怎麼報 usage，
以及——對帳號型上游——怎麼登入、重新整理和讀配額。

其餘都是別人的事。路由、憑證選擇、失敗轉移、協議轉換、結算和 capture 屬於引擎，而且
**渠道里沒有任何東西讀資料庫或挑傳輸後端**。

渠道是編譯進去的，不是插上去的：新渠道是 `gproxy-channel` 的一個模組，藏在它自己的 Cargo
feature 後面，新增一個意味著向本倉庫提一個 PR。預設不編譯任何具體渠道。

## 它需要一個渠道嗎

渠道是要對著別人的 wire 維護的程式碼。只有當一行 Provider 說不清它需要什麼時——路徑隨操作
移動、body 必須被改寫、有帳號 surface 要讀、有用戶端身份要呈現——一個廠商才值得一個渠道。

全部差別只是一個源和一個 header 的廠商，就是一個 `custom` Provider。見
[不需要渠道的廠商](/zh-tw/guides/providers/#接入相容服務)。

## 契約

`BaseChannel` 是唯一必需的 trait，`id` 是它唯一必需的方法。每個協議操作都有自己的非同步方法
並帶**預設實現**：用 `prepare`（HTTP）或 `prepare_connect`（WebSocket）構造請求，再把它交給
被指派的 client。

兩個準備鉤子的預設實現都是"不支援"，因此**一個渠道恰好支援它準備或覆寫的那些，絕不更多**。

```text
宿主选定 Provider、凭证、client
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
| `CredentialContext` | 重新整理、配額與服務用的 Provider、憑證和 client |

### 每個渠道都遵守的規則

- **準備是同步且純的**：不做 I/O、不藏狀態、不構造 client。操作覆寫可以做多次交換，但只能
  經 `context.client`，並且可以在回應流結束後繼續持有它完成收尾工作。
- **來源鑑權絕不到達上游。** 轉發助手丟棄逐跳 header，外加 `host`、`content-length`、
  `authorization`、`x-api-key`、`x-goog-api-key` 和 `api-key`；渠道從憑證里加上自己的
  鑑權。全域性白名單、Provider 的 `allowed_headers` 與渠道宣告的頭取並集，
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
| `usage_extractor` | 從緩衝的回應裡取歸一化用量。`None` 表示**未報告**，不是 0 |
| `usage_stream` | 按回應的觀察者，餵給它原始 chunk 或幀。它絕不改寫交付出去的流 |
| `services` | 廠商控制面路由——見 [CLI 用戶端](/zh-tw/guides/cli-clients/) |

跨請求記憶由宿主限定在一個 Provider 加一把憑證的範圍內，並以 CAS 寫入。binding 的預設實現
拒絕一切寫入，因此想要狀態的渠道必須被明確授予。

### 登入口袋

兩種 OAuth 流程都帶著同一個口袋，用來存渠道在使用者完成授權之後需要的事實：`start` 放進去
的東西會隨授權一起回來。它的形狀是渠道自己的——一個非標準的裝置控制代碼、一個登入過程為自己
註冊的 client。

**這個口袋可以帶金鑰**，與宿主會當作憑證 metadata 公開的 `provider_fields` 不同：它活在
宿主的登入會話裡，短命、由 cache 承載、從不被渲染，登入結束就沒了。任何必須活過登入的
東西都要從 exchange 返回——公開事實進 metadata，秘密進 provider secrets，後者與令牌一起
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
     config.rs     Provider 的 `config` JSON，serde(default)，忽略未知鍵
     request.rs    prepare / prepare_connect / 被覆寫的操作
     oauth.rs      登入與重新整理能力
     quota.rs      配額能力
     usage.rs      用量能力
     services.rs   廠商控制面路由
   ```

   **模組文件要點名每一條 wire 事實的來源**——廠商的 API 參考，或 `samples/` 下的 CLI
   原始碼。wire 真相絕不是另一個渠道的程式碼。

4. **實現 `BaseChannel`。** 最少是 `id`、`native_dialects` 和 `prepare`。結構體是無狀態
   單元，所以一個實例服務該渠道的每一個 Provider。

   也要覆寫 `descriptor`。預設實現只讀能力訪問器，而只有渠道自己知道它的顯示名，以及它
   從 Provider `config` JSON 裡解碼哪些鍵。**那份 descriptor 是管理 UI 渲染 Provider 表單
   的全部依據**——在 `ts` feature 下它還是一個 TypeScript 型別——所以漏掉的鍵就是沒人能設的鍵。

   當上遊需要多次交換、需要本地合成的應答、需要另一種傳輸，或需要把回應改寫成宣告的原生
   方言時，覆寫具體的操作方法而不是 `prepare`。

5. **宣告廠商用戶端會發什麼。** 如果渠道模擬某個 CLI，匯出它的 header 常量並用它構造
   allow-list，這樣 Provider 的 `allowed_headers` 就剝不掉 CLI 自己的 header。把這些身份
   header 傳進轉發助手的丟棄列表，讓**用戶端無法偽造它們**。上游對用戶端做指紋時返回一份
   預設連線配置；憑證或 Provider 上的顯式配置依然勝出。

6. **把能力做成獨立型別**並從訪問器返回。守住每個 trait 的規則：重新整理返回完整替換而不是
   合併；配額維度從憑證 metadata 讀套餐事實，絕不走網路；用量觀察者做累積快照，而它的
   `finish` 從**宿主**收到"完成還是被打斷"，因為光有 EOF 並不能確立完整的用量。

7. **可複用的 wire 機制放進 `src/channels/shared/`**，按使用它們的 feature 門控。策略留在
   渠道里；shared 模組只執行渠道要求的事。

8. **測試貼著程式碼放**在 `tests/<id>.rs`，用 `#![cfg(feature = "…")]` 門控。手工構造
   Provider 與憑證檢視，用一份 fixture 金鑰調 `prepare`，斷言 URL、注入的鑑權和被丟棄的
   header。把抓到的幀餵給用量觀察者；解析錄下來的配額 header 與用量 body。一個指令碼化的
   出站 client 用來演練多次呼叫覆寫。**不要從這裡測引擎。**

9. **在宿主裡註冊它。** 引擎從不自己列渠道。想要全部編譯進去的宿主就取整份列表——
   `gproxy-sdk` 正是這麼做的，所以在那邊開啟 feature 就是唯一一步——想要精確集合的則傳實例
   進去。重複 id 會被拒絕，而點名了未註冊渠道的 Provider 是一個配置錯誤，不是一次回退。

## 收尾

```sh
cargo fmt --all
cargo clippy -p gproxy-channel --all-features --all-targets -- -D warnings
cargo clippy -p gproxy-channel --all-features --target wasm32-unknown-unknown --lib -- -D warnings
cargo test   -p gproxy-channel --all-features
```

**每個渠道都要能為原生目標和 `wasm32-unknown-unknown` 構建**，這正是 Workers 宿主不是一個
縮水渠道集的原因。lint 報錯要改程式碼，不是加 `#[allow]`。

進去之後就不需要別的了：新渠道上的 Provider 會帶著它的 descriptor 出現在
`GET /admin/api/channels` 裡，管理 UI 從那份 descriptor 渲染它的表單。
