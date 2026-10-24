---
title: OrcaTerm
description: "透過 OrcaTerm 桌面端的 LightAI 協定接入帳號，支援瀏覽器 OAuth、權杖更新、帳號額度、動態模型目錄和文字對話。"
---

`orcaterm` 渠道使用 OrcaTerm 1.6.6 內建的 LightAI 協定，支援系統提示、多輪歷史、串流回答和思考內容。GPROXY 對外提供 OpenAI Chat Completions，並透過既有轉換支援 Responses、Claude Messages 和 Gemini。

## 接入步驟

1. 新增 **OrcaTerm** 供應商。中國站服務的基底位址留空即可；國際站帳號啟用 `international`。
2. 選擇預設的**裝置授權**登入方式，開啟授權連結並完成瀏覽器登入。GPROXY 自動輪詢並儲存 OAuth 憑證，無需複製裝置碼、Cookie 或回呼 URL。中國站預設微信，國際站預設 Google；透過 `oauth_provider` 可選擇 `wechat`、`txcloud`（騰訊雲）、`google` 或 `github`，實際可用方式由 OrcaTerm 所選區域決定。
3. 更新供應商的模型目錄。模型來自桌面端使用的動態 Rainbow 設定，請保留回傳的完整模型 ID，包括 `TokenHub/`、`Hunyuan3/` 等前綴。
4. 為匯入的模型分配存取權限，即可透過 GPROXY 傳送請求。

已有憑證透過 `POST /admin/api/credentials`（或批次介面）匯入，再透過 `POST /admin/api/credentials/{id}/refresh` 初始化。控制台儲存後自動重新整理待初始化的記錄；失敗後保留已儲存記錄，可在詳情中重試。Claude Web 使用 `authKind: "cookie"`；Claude Code 和 OrcaTerm 使用 `authKind: "oauth"`，也可提交 `secret: {"cookie": "..."}`。歷史類型標籤繼續相容讀取。初始化和重新整理將 secret、到期時間及可選公開 metadata 一起按版本 CAS 儲存，無需資料庫遷移或歷史回填。 `secret: {"cookie": "ot_session=...; skey=..."}` / `secret: {"access_token": "..."}`.

權杖更新與用戶端一致：呼叫 `OAuthRefreshToken` 並傳入目前的存取權杖，不虛構獨立的 refresh-token 授權流程。

`base_url` 覆寫 LightAI 服務位址，不含 `/assistant`；`site_url` 覆寫登入與更新權杖的帳號服務位址。預設值如下：

| 區域 | LightAI | 帳號服務 |
| --- | --- | --- |
| 中國站 | `https://lightai.cloud.tencent.com` | `https://orcaterm.com` |
| 國際站 | `https://lightai.tencentcloud.com` | `https://orcaterm.ai` |

## 帳號額度

憑證的額度查詢呼叫 `POST /cgi/api?action=DescribeUserUsageStats`，RPC 的 `data` 為 `{"feature":"hunyuan_ai"}`。它對應桌面端的 **AI 問答次數額度**，顯示已用量、上限、剩餘量和使用比例。上限 `-1` 表示不限額；缺少的計數保持未知。空回應、格式錯誤或其他功能的額度不會被當作零餘額。

2026-10-24 檢查的公開 `DescribeProductEditions` 介面標示：基礎版 1000 次/月，專業版 6000 次/月，試用與網域類方案存在按日額度。這些是公開方案定義，不是特定帳號的實測餘額，也不會寫死在額度查詢中。請求次數不會被轉換成資金餘額或 token 餘額。

僅當 AI 用量本身回傳帶明確時區的 `quotaStartDate` / `quotaEndDate` 時，才顯示對應週期邊界。不會根據無時區日期、訂閱到期時間或固定月度規則推算重置時間，也不會自動購買額度或執行額度重置。

該計數器未說明目前哪些 LightAI 模型共用額度，因此這裡僅作**觀測與顯示**。讀數會進入一般額度檢視，但不會據此在本地封鎖所有模型，也不會將閘道請求累加到未經確認的額度池。上游本身的額度耗盡回應仍照常處理。

匿名用量查詢回傳 `VERIFY_USER_FAILED`；個人實際剩餘量需要登入帳號，目前尚未完成真實帳號驗證。

## 支援範圍

目前僅支援**文字**。圖片、音訊、檔案內容、用戶端函式工具、工具歷史、強制工具選擇、結構化輸出和多候選回答都會被拒絕。上游要求呼叫 OrcaTerm 工具時會回傳錯誤；GPROXY 不會替呼叫者執行終端命令或核准工具操作。

每個請求使用新的上游對話 ID，並明確傳遞呼叫者提供的歷史，避免不同下游對話共用上游對話。這不是零保留模式，伺服器端歷史保留策略由 OrcaTerm 控制。渠道不匯入連線設定、SSH 憑證、終端錄製或桌面遙測。

已核對的 SSE 協定沒有可驗證的 token 計數欄位，因此用量保持「未回報」，不會偽裝成零。帳號額度單獨查詢，不宣告付費續用、額度重置或 WebSocket 能力。模型發現涵蓋內建模型，不會把帳號私有的第三方模型設定冒充成內建模型 ID。下游模型清單仍來自 GPROXY 本地、經過權限篩選的目錄。

2026-10-24 已驗證公開模型目錄和匿名 OAuth 入口：中國站微信、騰訊雲以及國際站 Google 均重新導向至對應授權頁面。未完成授權的 `OAuthExchangeToken` 回傳 `TOKEN_EXPIRED`，渠道在本地十分鐘登入時限內將它視為等待授權，其他錯誤不會被隱藏成等待狀態。

成功授權、生成與更新權杖仍需真實帳號，尚未完成帶認證的端到端實測。腳本化傳輸檢查僅驗證請求與回應處理，不證明帳號權益或真實上游呼叫成功。國際站位址選擇依據桌面端分支，並非另外擷取的國際版安裝實例。

## 協定依據

官方 Windows x64 標準版安裝套件：

```text
https://orcaterm-desktop-1258344699.cos.ap-guangzhou.myqcloud.com/release/latest/OrcaTerm-latest-win-x64.exe
SHA-256 f42940c099bc32fbe668adfea25fd5353f2b711b1e705536638aa1f78b28c173
```

已提取連續 PE 資源表中的全部 450 項並完成 Brotli 解壓縮。本地恢復的廠商程式碼保存在 `samples/orcaterm-1.6.6/`，該目錄不納入版本控制。關鍵 webpack 模組及格式化程式碼位置如下：

| 來源 | 協定事實 |
| --- | --- |
| 模組 `2481`，`callSubAgent` 與 `createCallHandler` | `/assistant/chat`、`input.historyMessages`、UUID、`name`、`mode`、`csrfCode` 和 SSE 請求標頭 |
| 模組 `26196` | `data:` 串流格式、內容和思考增量、`[DONE]`、`[ERROR]:...`、`[ABORTED]` |
| 模組 `82025`、`92054` | 輸入動作列舉和 `user.setting` 封裝 |
| `features.js:40277` | Agent 的 `completion.taskCompletion`、`ask.question`、`review.tool` 及思考內容對應 |
| `features.js:6660` | 各區域 LightAI 位址 |
| `features.js:14836` | `modelDesc.kwargs` 中的模型思考參數 |
| `features.js:818` | 公開 Rainbow 請求及 `orcaterm.内置模型` 分組 |
| `features.js:675` | 正權重模型記錄和按帳號選擇加權變體 |
| `app.js:6210`、`features.js:87360` | 瀏覽器授權 URL、工作階段 UUID 和桌面回呼交接 |
| `oauth-browser-redirects.json`、`oauth-pending-session.json` | 匿名實測的授權重新導向和待授權回應 |
| `ot_setting.js` 的 `Kt` / `Zt`；`features.js:94967`、`77060` | AI 用量請求、已用/上限、不限額標記和次數顯示 |
| `DescribeProductEditions-anonymous.json` | 公開方案週期（`update_span`、`update_unit`）和標示上限 |
| `DescribeUserUsageStats-anonymous.json` | 用量介面要求帳號認證 |
| `features.js:87182` | `GetUserInfo` 身分欄位 |
| `features.js:88116`、`88231`、`88310` | `ot_session`、OAuth 交換/更新及有效期 |
| `features.js:88370` | 明確的工作階段/更新權杖拒絕碼與暫時失敗重試 |
| `app.js:2920`、`2057` | 帳號 RPC 封裝與權杖更新動作 |
| `http-interceptor.js` | 桌面身分和 Cookie 注入；UA 字面值中的產品版本仍是 `1.0.0` |

無需憑證的公開模型目錄請求：

```http
POST https://dnspod.cloud.tencent.com/ajax/rainbow?action=getRainbowConfig
Content-Type: application/json

{"app":"ai","groups":["orcaterm.内置模型"],"options":{"enableEnv":1}}
```

回應中的 `model` 是實際傳給上游的模型選擇器，`displayName` 和 `text` 是顯示名稱。停用記錄（`weight=0`）會被篩除；發現失敗時不會以寫死的快照替代。
