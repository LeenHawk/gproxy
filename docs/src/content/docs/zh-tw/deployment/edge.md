---
title: "託管平台部署"
description: "在 Cloudflare Workers、Netlify 或 Vercel 上部署 GPROXY，選擇資料庫並完成首次登入。"
---

Northflank、Render 與 Heroku 的部署卡片及容器範本，請參閱[容器託管部署](/zh-tw/deployment/containers/)。

GPROXY 提供 Cloudflare Workers、Netlify 和 Vercel 的部署模板。模板下載預編譯的釋出包，不需要安裝 Rust。部署後，控制台和 API 使用同一個域名，控制台入口為 `/console/`。

## 選擇平台

| 平台 | 使用平台資料庫 | 使用已有資料庫 | 當前模板 WebSocket / Realtime |
| --- | --- | --- | --- |
| [Cloudflare Workers](#cloudflare-workers) | 部署時建立 D1 | libSQL / Turso | 支援 |
| [Netlify Functions](#netlify) | Netlify Database | PostgreSQL | 不支援 |
| [Vercel Functions](#vercel) | 部署時選擇 PostgreSQL 產品 | PostgreSQL | 支援（平台 Beta） |

只需要 HTTP 和 SSE 串流呼叫時，可選用任一模板；需要 WebSocket 或 Realtime 時，可選擇 Cloudflare Workers、Vercel Functions（WebSocket Beta），或使用 [CLI / 容器部署](/zh-tw/getting-started/installation/)。

Cloudflare 使用 WASM，另外三個平台使用原生 serverless 程式。它們共用 GPROXY 的管理 API 和控制台，但資料庫型別、函式時長和請求大小限制取決於平台。

## 準備密碼與主金鑰

每個平台都需要填寫以下兩項：

| 配置 | 填寫內容 |
| --- | --- |
| `GPROXY_ADMIN_PASSWORD` | 至少 8 個字元的登入密碼 |
| `GPROXY_MASTER_KEY` | 用於加密上游憑證的 32 位元組主金鑰；支援 64 位十六進位制或 base64 |

可以用 `openssl rand -hex 32` 生成主金鑰。儲存好這個值，更新和重新部署時繼續使用它。

使用者名稱預設為 `admin`，可透過 `GPROXY_ADMIN_USER` 指定。密碼配置會在啟動時應用：優先更新同名使用者的密碼；沒有同名使用者時恢復 0 號管理員並修改使用者名稱和密碼。詳見[管理員初始化與密碼覆蓋](/zh-tw/reference/configuration/#首次初始化)。

使用已有資料庫時，還需要資料庫連線資訊。各平台的填寫方式如下。

## Cloudflare Workers

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)][cloudflare-auto]

[使用外部 libSQL / Turso 資料庫][cloudflare-external]

使用 D1 點部署按鈕，已有 libSQL/Turso 資料庫則點外部資料庫連結。

1. 進入所選部署入口，連線 GitHub 或 GitLab 帳戶。
2. 使用 D1 時，填寫資料庫名稱，Cloudflare 會建立並繫結它。使用外部資料庫時，填寫 `GPROXY_DATABASE_URL` 和 `GPROXY_LIBSQL_TOKEN`；這個入口不會建立 D1。
3. 填寫管理員密碼和主金鑰，保留 `npm run build`、`npm run deploy` 命令，然後部署。

Turso 地址可使用 `libsql://your-db.turso.io` 或對應的 HTTPS 地址。這裡使用 libSQL 的 HTTP 介面，不能填寫 PostgreSQL 連線串。

已有 D1 部署也可以透過上述兩個變數改用 libSQL。設定 `GPROXY_DATABASE_URL` 後，它優先於 D1；更改連線只切換資料庫，不會搬遷原有資料。

需要使用 Wrangler 或調整 Workers 配置時，見本頁的[手動部署 Cloudflare](#手動部署-cloudflare)。

## Netlify

[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)][netlify-auto]

[使用已有 PostgreSQL 資料庫][netlify-external]

使用 Netlify Database 點部署按鈕，已有 PostgreSQL 則點外部資料庫連結。

1. 進入所選部署入口，授權倉庫，並填寫管理員密碼和主金鑰。
2. 使用 Netlify Database 時，模板讀取平台提供的連線，無需手填連線串。使用已有資料庫時，在 `GPROXY_DATABASE_URL` 中填寫 PostgreSQL 連線串。
3. 保留模板的構建命令和 Functions 配置，開始部署。

外部連線的格式例如 `postgresql://user:password@db.example.com/gproxy?sslmode=require`，請替換為資料庫服務提供的實際連線資訊。

兩個入口使用同一個 Netlify 模板。即使填寫外部連線，Netlify 仍可能建立平台資料庫；GPROXY 會優先使用 `GPROXY_DATABASE_URL`。

## Vercel

[![Deploy with Vercel](https://vercel.com/button)][vercel-auto]

[使用已有 PostgreSQL 資料庫][vercel-external]

需要新資料庫時點部署按鈕，已有 PostgreSQL 則點外部資料庫連結。

1. 進入所選部署入口。使用平台資料庫時，按頁面提示選擇並關聯 PostgreSQL 產品；外部資料庫入口不要求安裝資料庫產品。
2. 填寫管理員密碼和主金鑰。使用已有資料庫時，再填寫 `GPROXY_DATABASE_URL`。
3. 保留模板的構建命令和 `vercel.json`，開始部署。

平台資料庫連線從 `DATABASE_URL` 或 `POSTGRES_URL` 讀取。如果所選產品使用其他變數名，把連線串填入 `GPROXY_DATABASE_URL`。

這個模板使用 **Node.js Functions**，部署時不要改為 Edge Runtime。

## 首次登入與呼叫

部署完成後，開啟 `https://你的域名/console/`，使用 `admin`（或自定義使用者名稱）和配置的密碼登入。

在控制台新增供應商與憑證，測試成功後建立模型路由和閘道器 API Key，再按[第一個請求](/zh-tw/getting-started/first-request/)發起呼叫。用戶端使用部署域名作為服務地址，例如 `https://gateway.example.com/v1`，使用閘道器 API Key 鑑權。

如果 Netlify 或 Vercel 返回 503，先檢視函式日誌，確認資料庫已繫結、連線串有效，以及管理員密碼和主金鑰已設定。這些平台的 PostgreSQL 帳戶還需要建立 `gproxy` schema 和應用表的權限。

## 更新與執行限制

設定、帳戶與用量儲存在資料庫中，更新時保留資料庫與主金鑰。範本預設下載最新穩定版並驗證 SHA-256；升級時清除平台建置快取後重新部署即可。只有需要固定版本時，才將建置環境變數 `GPROXY_RELEASE_VERSION` 設為發行標籤。所選版本需包含對應平台的發行附件。託管部署不使用控制台的原地二進位更新。

所有模板均支援 HTTP 與 SSE；Netlify Functions 不接受 WebSocket 升級。Vercel 使用平台的 [WebSocket Beta](https://vercel.com/docs/functions/websockets)。WebSocket 連線仍可能因函式時長限制或實例回收而斷開，用戶端需要處理重連。長時間推理、大檔案和高併發請求仍受平台限制，部署前可檢視 [Netlify Functions](https://docs.netlify.com/build/functions/overview/) 和 [Vercel Functions](https://vercel.com/docs/functions/limitations) 的當前額度。

這兩個模板不配置檔案儲存。需要 S3/R2 或可公開下載的檔案時，可以使用下方的 Workers 自定義構建，或選擇 CLI 部署。Netlify 和 Vercel 可用 `GPROXY_PUBLIC_BASE_URL` 固定公開訪問地址。不設定時，Netlify 和 Vercel 使用平臺提供的生產地址；其他平臺的 OAuth 端點按每個請求的 Host 應答，釋出連結保持停用。

## 手動部署 Cloudflare

### 使用釋出包

下載 `gproxy-edge-cloudflare.zip`，解壓並進入 `cloudflare` 目錄。包內包含 Worker、控制台和 `wrangler.toml`。

使用 D1 時，安裝依賴並建立資料庫：

```sh
pnpm install
pnpm exec wrangler d1 create gproxy
```

把返回的資料庫 ID 填入 `wrangler.toml`，保留 binding 名 `DB`：

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "replace-with-your-database-id"
```

設定主金鑰和管理員密碼，再部署：

```sh
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler secret put GPROXY_ADMIN_PASSWORD
pnpm exec wrangler deploy --dry-run
pnpm exec wrangler deploy
```

使用 libSQL 時，設定 `GPROXY_DATABASE_URL` 和 `GPROXY_LIBSQL_TOKEN`，並移除不使用的 D1 binding。釋出包已啟用 libSQL；自行構建時需保留對應 feature。資料庫表由 GPROXY 首次啟動時建立，無需單獨執行 Wrangler SQL migrations。

### Workers 配置與靜態資源

`GPROXY_CONFIG` 接受 JSON 配置，具名 Secrets 覆蓋對應欄位。以下為 D1 與公開訪問地址的配置示例：

```toml
[vars]
GPROXY_CONFIG = '{"store":{"kind":"d1","binding":"DB"},"cache":{"kind":"store"},"public_base_url":"https://gateway.example.workers.dev"}'
```

控制台由 Workers Assets 提供。保留模板中的路由配置，讓 `/console/` 使用靜態資源，其他請求進入閘道器：

```toml
[assets]
directory = "./public"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/*", "!/", "!/console", "!/console/*"]
```

Workers 支援 D1 或 libSQL，不使用本地 SQLite、TCP 資料庫或本地檔案目錄。需要 S3/R2 時，構建時啟用 `s3` feature，並設定 `GPROXY_S3_ACCESS_KEY_ID`、`GPROXY_S3_SECRET_ACCESS_KEY` 和相應的檔案儲存配置。完整欄位見[配置參考](/zh-tw/reference/configuration/)。

### 從原始碼構建

從倉庫根目錄執行：

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

構建與打包選項見[從原始碼構建](/zh-tw/deployment/release-build/)。

[cloudflare-auto]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-button
[cloudflare-external]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-external
[netlify-auto]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless
[netlify-external]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless#GPROXY_DATABASE_URL=
[vercel-auto]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY&products=%5B%7B%22type%22%3A%22integration%22%2C%22group%22%3A%22postgres%22%2C%22protocol%22%3A%22storage%22%7D%5D
[vercel-external]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY%2CGPROXY_DATABASE_URL
