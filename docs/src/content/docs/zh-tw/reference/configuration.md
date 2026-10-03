---
title: "配置"
description: "CLI 配置來源、環境變數、主金鑰、首次初始化與執行時設定。"
---

GPROXY **只在啟動時讀一次**程序配置。入口點之下沒有任何模組讀環境變數。

一切在程序執行期間會變的東西——Provider、憑證、路由、改寫規則、價格、身份，以及本頁末尾
的 settings 行——都在資料庫裡。

## 五個來源

從強到弱：

1. **命令列**——`--port 9000`
2. **真實環境**——`GPROXY_PORT=9000`
3. **`.env`**，載入時不覆蓋環境已經設過的任何東西
4. **`--config` 點名的 TOML 檔案**
5. **內建預設值**

環境變數可以覆蓋配置檔案，便於不同主機或容器複用同一份配置。

每個配置 flag 都是全域性的，因此 `gproxy --port 9000 serve` 和 `gproxy serve --port 9000`
是同一次呼叫。

## 環境變數

下面列出常用環境變數。`gproxy --help` 顯示當前二進位制支援的引數及對應變數。

| 變數 | Flag | 預設 | 是什麼 |
| --- | --- | --- | --- |
| `GPROXY_CONFIG` | `--config`、`-c` | — | 裝任意配置欄位的 TOML 檔案 |
| `GPROXY_HOST` | `--host` | `127.0.0.1` | 監聽地址（一個 IP，不是主機名） |
| `GPROXY_PORT` | `--port`、`-p` | `8787` | 監聽連接埠 |
| `GPROXY_DATA_DIR` | `--data-dir` | `data` | 相對路徑相對它解析的根 |
| `GPROXY_PERSISTENCE` | `--persistence` | `sqlite` | `sqlite`、`postgres` 或 `mysql` |
| `GPROXY_DSN` | `--dsn` | — | 連線串；沒有 `--persistence` 時由它自己點明後端 |
| `GPROXY_REDIS_URL` | `--redis-url` | — | 共享 cache；**多實例必需** |
| `GPROXY_MASTER_KEY` | `--master-key` | — | 32 位元組，64 個十六進位制字元或 base64。不設則明文存放 |
| `GPROXY_MASTER_KEY_NEXT` | `--master-key-next` | — | 要重新密封到的那把鑰匙 |
| `GPROXY_MASTER_KEY_ROTATE` | `--master-key-rotate` | `false` | 啟動時執行輪換 |
| `GPROXY_PUBLIC_BASE_URL` | `--public-base-url` | — | 對外源，用於釋出連結和 OAuth issuer 標識 |
| `GPROXY_CORS_ORIGINS` | `--cors-origin` | — | 逗號分隔的瀏覽器 origin；空表示僅同源 |
| `GPROXY_TRUSTED_PROXIES` | `--trusted-proxy` | — | 逗號分隔、其 `x-forwarded-*` 可被相信的對端；空表示誰都不信 |
| `GPROXY_FILE_STORAGE_DIR` | `--file-storage-dir` | — | 釋出 body 與詞表的本地目錄 |
| `GPROXY_AUDIT_ENABLED` | `--audit-enabled` | `true` | 記錄管理及 OAuth 審計；設為 `false` 關閉新增記錄 |
| `GPROXY_CONSOLE` | `--console` | `true` | 提供 console |
| `GPROXY_CONSOLE_PATH` | `--console-path` | — | 從這個目錄而不是內嵌 bundle 提供 console |
| `GPROXY_INSTANCE_ID` | `--instance-id` | 隨機 | 本程序的穩定名字 |
| `GPROXY_LOG_FORMAT` | `--log-format` | `text` | `text` 或 `json` |
| `GPROXY_LOG_FILTER` | `--log-filter` | `RUST_LOG`，再 `info` | tracing 過濾器 |
| `GPROXY_ADMIN_USER` | `--admin-user` / `--user` | `admin` | 第一個管理員的名字 |
| `GPROXY_ADMIN_PASSWORD` | `--admin-password` / `--password` | 生成 | 他的密碼 |
| `GPROXY_BOOTSTRAP_ADMIN_API_KEY` | `--admin-api-key` / `--api-key` | 生成 | 要為他簽發的那把確切 API key |
| `GPROXY_IMPORT_SOURCE_MASTER_KEY` | `--source-master-key` | — | 僅 `import`：源實例的鑰匙 |
| `GPROXY_ENV_FILE` | — | `.env` | 載入哪個 `.env` |
| `GPROXY_UPDATE_CHANNEL` | `--update-channel` | 構建渠道 | `dev`、`beta` 或 `release` |
| `GPROXY_UPDATE_SOURCE` | `--update-source` | 構建來源 | `github`、`gitlab` 或 `cnb` |
| `GPROXY_UPDATE_MANIFEST_URL` | `--update-manifest-url` | 來源與渠道對應 URL | 自定義簽名更新清單 |
| `GPROXY_UPDATE_RESTART` | `--update-restart` | `re-exec` | 更新後 `none`、`supervisor`（退出碼 42）或 `re-exec` |
| `GPROXY_UPDATE_CHECK_INTERVAL` | `--update-check-interval` | `21600` | 檢查間隔秒數，`0` 關閉定時檢查 |
| `GPROXY_UPDATE_AUTOMATIC` | `--update-automatic` | `false` | 自動安裝檢查到的更新 |
| `GPROXY_UPDATE_VERIFY_SIGNATURE` | `--update-verify-signature` | `true` | 校驗更新清單簽名；設為 `false` 跳過簽名校驗，仍校驗安裝包大小和 SHA-256 |
| `GPROXY_AUTOSTART` | `service install --autostart` | 見命令幫助 | 安裝服務時的啟動選項 |

Console → 系統更新 → 校驗更新簽名會立即儲存實例設定，重啟後保留。透過設定 API 將 `instance.updateVerifySignature` 設為 `null`，可恢復跟隨命令列/環境變數預設值。

`GPROXY_ENV_FILE` 刻意沒有 flag：flag 得由它所餵養的那一步來解析，因此它影響不了解析器
自己讀到的值。

布林值接受 `1`、`true`、`yes`、`on`、`0`、`false`、`no`、`off`。**拼錯是錯誤而不是
`false`**。

關閉審計：設定 `GPROXY_AUDIT_ENABLED=false`、傳入 `--audit-enabled false`，或在 TOML 頂層設定 `audit_enabled = false`，然後重啟。歷史審計仍可查詢；請求日誌、用量統計不受影響。防止重複匯入的遷移完成標記仍會儲存。

## 命令

| 命令 | 做什麼 |
| --- | --- |
| `gproxy serve` | 按需輪換主金鑰、實例是新的就 bootstrap、繫結、服務。**預設命令**——不帶子命令的 `gproxy` 就是 `gproxy serve`。 |
| `gproxy migrate` | 建立或增量同步表結構，然後退出。 |
| `gproxy bootstrap admin` | 建立第一個管理員。冪等。 |
| `gproxy export --out <PATH>` | 把本實例的配置寫成一份 JSON 文件。 |
| `gproxy import --in <PATH>` | 把這樣一份文件回放進本實例。 |
| `gproxy update --check` | 僅檢查更新，不安裝。 |
| `gproxy update` | 安裝可用更新。 |
| `gproxy service --help` | 檢視當前平台的服務安裝與管理選項。 |

`serve` 在 `SIGINT` 或 `SIGTERM` 上停止，先把在途請求排空。**沒有關停超時**：一次串流
補全跑上幾分鐘是合法的，而想要截止時間的 supervisor 自己有。

繫結發生在表結構工作**之後**，因此一個還在遷移的程序直接拒絕連線，而不是把它們收進一個
沒人應答的積壓佇列。

## 配置檔案

檔案講的是配置型別自己的欄位名，`snake_case`。**未知鍵是錯誤**，因此拼寫錯誤在啟動時就被
報出來，而不是被靜默忽略。

```toml
host = "0.0.0.0"
port = 8787
audit_enabled = true
data_dir = "/var/lib/gproxy"
public_base_url = "https://gproxy.example.com"
cors_origins = ["https://console.example.com"]
trusted_proxies = ["10.0.0.0/8"]
session_ttl_secs = 2592000

[store]
kind = "sqlite"
path = "gproxy.db"
# kind = "url"
# dsn = "postgres://gproxy@db/gproxy"

[cache]
kind = "memory"
# kind = "redis"
# url = "redis://cache:6379"
# namespace = "prod"

[master_key]
rotate = false

[master_key.key]
kind = "hex"
value = "0000000000000000000000000000000000000000000000000000000000000000"

[file_storage]
kind = "fs"
root = "files"
# kind = "s3"
# bucket = "gproxy"
# region = "auto"
# endpoint = "https://…"

[console]
enabled = true

[oauth]
access_ttl_secs = 3600
refresh_ttl_secs = 2592000
code_ttl_secs = 300
device_ttl_secs = 900
cli_client_ids = []
```

有幾個欄位沒有 flag，因為它們不是容器會去覆蓋的東西：`session_ttl_secs`、整個 `[oauth]`
塊，以及 S3 細節。

這就是 Workers 宿主在 `GPROXY_CONFIG` 裡讀作 JSON、桌面宿主在資料目錄裡讀作 `gproxy.toml`
的**同一份文件**，所以沒有第二套 schema 要維持同步。

## 靜態金鑰

憑證金鑰、被保留的 API key 和 tokenizer 源令牌，都用主金鑰以 AES-256-GCM 密封。
**密封繫結到該行自己的 id**，所以一份密文被拷到另一行上打不開。

沒有主金鑰時金鑰以明文存放。這是一種受支援的部署形態，不是意外，而且二進位制在啟動時會
大聲說一次，並點名那個能修好它的變數：

```text
WARN gproxy::serve: upstream credential secrets are stored UNENCRYPTED: no
     master key is configured. Set GPROXY_MASTER_KEY to 32 bytes as 64 hex
     characters or base64 — and, on a database that already holds secrets, set
     GPROXY_MASTER_KEY_NEXT with GPROXY_MASTER_KEY_ROTATE to seal what is
     already there.
```

用 `openssl rand -hex 32` 生成一把，正好 64 個十六進位制字元。32 位元組的 base64 是 43 或 44
個字元，所以兩種編碼不會混淆，格式是嗅探出來的而不是配置出來的。

### 主金鑰輪換

換鑰匙不是改一行配置。每一份密文都必須用舊鑰匙開啟、再用新鑰匙封上，否則下一次過載會在
它打不開的第一把憑證上失敗。所以輪換是一次真正的操作，在啟動時執行：

```sh
# 1. 停掉每一个实例。
# 2. 带着轮换开关启动一个。
GPROXY_MASTER_KEY=<old> \
GPROXY_MASTER_KEY_NEXT=<new> \
GPROXY_MASTER_KEY_ROTATE=true \
  gproxy serve

# 3. 它会在 WARN 级别记录：
#    master key rotated; copy GPROXY_MASTER_KEY_NEXT to GPROXY_MASTER_KEY,
#    then clear GPROXY_MASTER_KEY_NEXT and GPROXY_MASTER_KEY_ROTATE

# 4. 把新钥匙提升上来，清掉另外两个。
GPROXY_MASTER_KEY=<new> gproxy serve
```

三條性質讓它敢跑：

- **一個事務。** 每一次更新和 revision 自增一起提交，所以失敗只會讓資料庫整體停在舊鑰匙
  上，重試即可。絕不會出現一個半輪換的資料庫要先搞清楚它是什麼形狀。
- **寫之前先全部開啟。** 一把能解開大多數行的鑰匙，會在第一次寫入之前就中止。
- **什麼都不跳過。** 打不開的密文是錯誤，絕不是一行被留下——被跳過的行在運維者提升新鑰匙
  之後，就是**兩把**鑰匙都打不開了。

由此得出兩條規則：

- **輪換時只跑一個實例。** 資料庫現在在哪把鑰匙上沒有持久記錄，所以一個還拿著舊鑰匙的
  同伴會看到 revision 自增、過載，然後什麼都打不開。
- **執行輪換的程序此後一直用新鑰匙服務**，因為資料庫現在就裝著那個。忘了第 4 步意味著
  下次啟動什麼都打不開——大聲地、在裝配時，而不是靜默地。

`GPROXY_MASTER_KEY_NEXT` 配上沒設的 `GPROXY_MASTER_KEY` 是**採納路徑**：它給一個本來在跑
明文的資料庫加封。

設了 `GPROXY_MASTER_KEY_NEXT` 卻*沒有* `GPROXY_MASTER_KEY_ROTATE` 時什麼也不做，並且會
這麼說——這樣一把在部署裡躺了一個月的"下一把鑰匙"，不會被誤當成一次已經發生過的輪換。

## 首次初始化

CLI 在 users 表為空時建立 0 號管理員和閘道器 API Key。已有實例啟動時，`GPROXY_ADMIN_PASSWORD`（或 `--admin-password`）按以下規則覆蓋：優先查詢 `GPROXY_ADMIN_USER` 指定的同名使用者，只更新其密碼；若不存在同名使用者，則啟用 0 號管理員並更新使用者名稱和密碼，0 號使用者不存在時建立它。未發生變更時保留會話，密碼變化或恢復 0 號使用者時登出目標使用者的會話。未顯式設定密碼時不修改帳戶，重啟不會生成新的 API Key。

空庫初始化時，`GPROXY_ADMIN_PASSWORD` 和 `GPROXY_BOOTSTRAP_ADMIN_API_KEY` 可指定初始值；省略時自動生成。程式只顯示生成的憑據，輸出到標準輸出。服務管理器和容器可能收集標準輸出，因此首次啟動日誌同樣需要保管。

Application 使用[首次設定精靈](/zh-tw/getting-started/installation/#application在介面中設定)，不使用瀏覽器初始化頁。

## 搬運一份配置

```sh
gproxy export --out config.json --include-secrets
gproxy import --in  config.json --mode merge --source-master-key '…'
```

`-` 表示標準輸出或標準輸入。文件就是管理 DTO 本身，所以匯出說的正是一次列表會說的：

```json
{"formatVersion":5,"exportedAtMs":1789981723118,"secretsOmitted":true,
 "secrets":[],"data":{"connectionProfiles":[],"providers":[…],"credentials":[…],
 "models":[],"providerModels":[],"routes":[],"routeMembers":[],
 "operationRules":[],"operationEndpoints":[],
 "rewriteRuleSets":[],"rewriteRules":[],"providerRewriteRuleSets":[],
 "quotas":[],"priceRules":[],"priceRates":[],"priceTiers":[],"settings":null}}
```

`data` 按回放順序排列：沒有任何一行出現在它所指向的那一行之前。

**身份不走**——使用者、key、組織、團隊、權限、訂閱和 OAuth 用戶端屬於產品層——所以被匯入的
實例仍然需要它自己的 bootstrap。用量和 capture 也不走：複製它們等於偽造目的端從未有過的
歷史。

`--mode merge` 寫文件點名的東西，其餘不動。`--mode replace` 還會刪掉文件沒提到的、屬於
已匯出種類的行，先子後父，並且絕不碰身份、用量或 capture 表。一次匯入是**整份文件一個
revision 提交**，因此任何一處被拒絕的文件什麼也不會留下。

帶 `--include-secrets` 時密文以 base64 隨行，從不被開啟、也從不是明文——這份文件因此與
資料庫檔案同等敏感。回來的時候：

| 匯入方有什麼 | 會發生什麼 |
| --- | --- |
| `--source-master-key` | 每個金鑰被開啟並用本實例的鑰匙重新密封 |
| 與源相同的鑰匙 | 密文原樣存放，本來就能開啟 |
| 都沒有 | 這把憑證被跳過、計數並告警 |

這條規則存在，是因為引擎在裝配快照時會開啟每一把憑證的金鑰：一把匯入後打不開的憑證不會
只是壞掉它自己的呼叫，它會壞掉**整個實例此後每一次過載**。

## settings 行

控制台的「系統 → 全域性設定」直接修改該行；只提交已修改欄位，不覆蓋其他設定。
實例名稱顯示在側欄及網頁標題，版本和完整 Git hash 可從 `/info` 讀取。

- `corsOrigins`、`trustedProxies` 按當前快照讀取；啟動值只在新建 settings 行時初始化。
- 原生 `gproxy serve` 跟隨資料庫更新程序日誌級別／格式、更新通道和自動檢查開關。
- `enableTokenizerVocabs=false` 停用自定義詞表並回退內建計數；不刪除詞表檔案。
- `enableTokenizerDownload=false` 拒絕新下載；已有詞表是否使用由前一個開關控制。
- `retentionDays` 清理超過保留期的已結束請求歷史、抓包及其事件；未設定時不按時間清理。
- `maxDatabaseSizeMb` 以 MiB 限制 SQLite 歷史資料佔用，從最舊的已完成記錄開始清理並回收空頁。
  未設定或 0 不啟用此限制。配置和進行中的請求不刪除，因此該值不是整個資料庫的硬配額。
  自動清理由原生服務每分鐘執行；已結算賬目、配額計數、配置及審計記錄保持不變。
- `defaultFileStorageName`、`maxInFlight`、`fileUploadMaxInFlight` 已移除，不是可設定項。


執行期設定在資料庫裡，改了不用重啟。

```sh
curl -s http://127.0.0.1:8787/admin/api/settings  -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X PATCH http://127.0.0.1:8787/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"instance":{"maxAttempts":4}}'
```

一行，兩組。instance 組：

| 鍵 | 預設 | 含義 |
| --- | --- | --- |
| `instanceName` | `default` | 控制台名稱 |
| `maxAttempts` | `6` | 一個計劃上游嘗試次數的硬上限；路由自己的預算被它鉗住 |
| `requestTimeoutMs` | `1200000` | 等上游開始應答（拿到回應頭，或讀完一個轉換過的非串流應答）；持續有輸出的流不受它限制 |
| `streamIdleTimeoutMs` | `300000` | 流單元之間的間隔 |
| `maxRequestBodyBytes` | `52428800` | 除檔案上傳外的所有請求體，在鑑權之前檢查；壓縮請求體解壓後也不能超過它 |
| `maxUploadBodyBytes` | `536870912` | 檔案上傳的請求體 |
| `maxResponseBodyBytes` | `268435456` | 緩衝回應上限 |
| `maxStreamEventBytes` | `33554432` | 一個 SSE 事件或陣列元素 |
| `maxWsFrameBytes` | `33554432` | 更大的幀以 `1009` 關閉雙方 |
| `maxMultipartParts` | `64` | |
| `enableSettlement`、`enableUsage` | `true` | 定價與用量行 |
| `enableTokenizerVocabs` | `true` | 用真實詞表計數 |
| `enableTokenizerDownload` | `false` | 抓取未快取的詞表 |
| `retentionDays`、`maxDatabaseSizeMb` | 未設 | |
| `portalRecentRequestsEnabled` | `true` | 使用者面是否展示近期請求 |
| `corsOrigins`、`trustedProxies`、`connectionProfileId`、`oauthClientAllowlist` | | 同一批策略在資料庫裡的那一份 |
| `configRevision` | | 只讀：每個實例據以同步的 revision |

logging 組是 `enableDownstreamLog`、`enableDownstreamLogBody`、`enableUpstreamLog`、
`enableUpstreamLogBody`、`disableLogRedaction`、`enableTracing`、`logLevel`、`logFormat`
和三個黑名單。兩個 body 開關預設關閉，見
[用量、日誌與審計](/zh-tw/guides/observability/#請求日誌)。

## 跑多個實例

三件事會變：

1. **`GPROXY_REDIS_URL`。** 預設 cache 是程序本地的，兩個實例不會看到彼此的失效通知，
   也會各自獨立地計限流。回答不了的 cache 會**拒絕**一個受限流的請求而不是放行它——靜默
   地退回本地，會把一次故障變成"這個實例上所有限額都關了"。
2. **`gproxy migrate` 作為獨立一步**，由單一寫入者在任何實例啟動前執行。
3. **輪換時只跑一個實例**，如上。

## 可信代理規則

`x-forwarded-for` 和 `x-forwarded-proto` 是 header，而 header 是對端寫什麼就是什麼。
**只有當 socket 的對端是迴環或列在 `trustedProxies` 裡時**它們才被相信。來自任何其他對端
時直接忽略——不合並、不偏好、也不當兜底——而預設誰都不信。

兩者都要緊，理由不同。偽造的 `x-forwarded-for` 決定運維者日誌裡某個請求旁邊的那個地址。
偽造的 `x-forwarded-proto` 決定 **OAuth issuer 標識的 scheme**，而那是一份告訴用戶端把
授權碼發往哪裡的發現文件。

對端未知時按不可信處理。
