---
title: "Provider 與憑證"
description: "選擇渠道、新增供應商與憑證，配置登入、額度查詢和連線選項。"
---

**渠道（channel）** 是編譯進二進位制的某一族上游介面卡。**Provider** 是某個渠道上的一條
已儲存連線：名字、可選 base URL、該渠道自己的 `config` JSON，以及一個憑證池。同一個渠道
可以建任意多個 Provider——`openai-main` 和 `openai-eu` 可以都在 `openai` 渠道上。

供應商和憑證的配置儲存後生效，通常不需要重啟。首次接入請先測試憑證，再把它加入模型路由。

## 支援的渠道

只有編譯進你的二進位制的渠道才存在。`GET /admin/api/channels` 回答自二進位制而非資料庫，
每一項帶著登入方式、能力，以及一個 Provider 表單該渲染的 `config` 鍵。

| 渠道 | 上游 | 憑證 |
| --- | --- | --- |
| `aistudio` | Google AI Studio：原生 Gemini 方法與 `/v1beta/openai` 相容層同源 | `{"api_key"}` |
| `antigravity` | 經 Antigravity 編輯器所用的 Code Assist 主機訪問 Google 帳號 | OAuth |
| `aws_bedrock` | AWS Bedrock：按模型選介面——Anthropic 模型走 `InvokeModel`（event-stream 翻成 Claude SSE），GPT、Grok、Qwen、DeepSeek 等走 OpenAI 相容的 Chat Completions | AWS 金鑰對，或 Bedrock API key |
| `azure` | Azure OpenAI，以及 Azure AI Foundry 託管的 Anthropic 模型 | `{"api_key"}` |
| `claudeapi` | Anthropic 官方 API，加上它的 OpenAI 相容層與成本報表 | `{"api_key", "quota_api_key"}` |
| `claudecode` | 經 Claude Code CLI 的請求使用 Claude.ai 訂閱 | OAuth |
| `claudeweb` | claude.ai 瀏覽器會話，渲染成 Claude Messages SSE | 會話 cookie + 組織 |
| `cline` | Cline 自家帳號，`api.cline.bot` | `{"api_key"}` 或 OAuth |
| `cloudflare_ai_gateway` | 經 REST API 使用 Cloudflare AI Gateway；account 與 gateway 按憑據存放，額度餘額 | `{"api_key", "account_id", "gateway_id"?}` |
| `codex` | 經 Codex 後端使用 ChatGPT 帳號：HTTP SSE 與 WebSocket 上的 Responses、`/wham/usage` | OAuth |
| `copilotcli` | 經 `copilot` CLI 使用 GitHub Copilot | GitHub OAuth 令牌 |
| `custom` | 任何原生講 OpenAI、Claude 或 Gemini 的 API-key 端點 | `{"api_key"}` |
| `dashscope` | 阿里 DashScope：OpenAI 模式、Anthropic 模式、rerank 與原生多模態影象 API | `{"api_key"}` |
| `deepseek` | DeepSeek：`/v1` 下的 Chat、根路徑的 Responses、`/anthropic` 下的 Claude Messages | `{"api_key"}` |
| `glm` | GLM 按量 API；ZCode 官方動態模型目錄 | `{"api_key"}` |
| `glmcode` | GLM Coding Plan；獨立模型目錄、Chat／Messages／Responses、訂閱額度查詢 | `{"api_key"}` |
| `minimax` | MiniMax 文字 API／Token Plan，以及 H3 影片建立、查詢、列表、取消／刪除、下載 | `{"api_key"}` |
| `devin` | Devin（Windsurf），`server.codeium.com`：protobuf 上的 Connect-RPC | 會話令牌 |
| `geminicli` | 經 Gemini CLI 所用的 Code Assist 端點訪問 Google 帳號 | OAuth |
| `grokbuild` | 經 Grok Build CLI 使用 xAI 帳號 | OAuth |
| `kimi` | Moonshot 平台（API key），或 Kimi Code 訂閱（裝置登入） | `{"api_key"}` 或 OAuth |
| `kiro` | 經 Kiro 桌面應用使用 AWS CodeWhisperer | OAuth |
| `nvidia` | NVIDIA NIM：Chat Completions、模型列表與 embeddings | `{"api_key"}` |
| `openai` | OpenAI 官方平台：完整 surface、套接字上的 Responses 與 Realtime | `{"api_key", "quota_api_key"}` |
| `opencodego` | OpenCode Go：訂閱制、開源模型、用量視窗 | `{"api_key"}` |
| `opencodezen` | OpenCode Zen：從 Console 餘額按量付費 | `{"api_key"}` 或 OAuth |
| `openrouter` | OpenRouter：body 裡的路由偏好、應答裡報出的價格 | `{"api_key"}` |
| `vercel` | Vercel AI Gateway：模型呼叫與團隊餘額查詢 | `{"api_key"}` |
| `vertex` | Google Vertex AI：Google、Anthropic 與 OpenAI 相容釋出者 | 服務帳號金鑰 |
| `vertexexpress` | Vertex AI Express：單一全球源上的 Gemini surface | `{"api_key"}` |
| `workbuddy` | 經編輯器外掛使用騰訊 Copilot | OAuth |
| `xai` | xAI（Grok）：OpenAI Chat 與 Responses，加上 xAI 自己的 TTS／STT／影片 | `{"api_key"}` |

每個渠道都能為原生目標**和** `wasm32-unknown-unknown` 構建，所以 Worker 部署不是一個
縮水的渠道集。

### 接入相容服務

相容現有協議的服務可以透過 `custom` 接入，填寫服務地址和支援的協議即可：

```json
{ "name": "example-vendor", "channel": "custom",
  "baseUrl": "https://api.example-vendor.com",
  "config": { "dialects": ["openai_chat"] } }
```

NVIDIA NIM、Vercel AI Gateway 和 Cloudflare AI Gateway 提供專用渠道，優先使用對應渠道以獲得專用的認證和額度查詢功能。

## 供應商配置

| 欄位 | 含義 |
| --- | --- |
| `name` | 運維者取的標籤，唯一。它同時是 **Provider 掛載點**和 `provider/model` 形式的左半邊。 |
| `channel` | 上表中的一個 id。 |
| `baseUrl` | 渠道接受 base URL 時的源。 |
| `config` | 該渠道自己的 JSON。每個渠道宣告它解碼哪些鍵，`GET /admin/api/channels` 就是那份清單。 |
| `connectionProfileId` | 這個 Provider 的呼叫走哪套出站棧——代理、TLS 模擬。 |
| `enabled` | 被禁用的 Provider 退出解析。 |

`custom` 的 `config` 帶 `dialects`（端點講哪些協議格式）、靜態 `headers`、`allowed_headers`
和兩個 magic cache 開關。模擬廠商 CLI 的渠道宣告自己的身份 header，並且不讓用戶端偽造它們。

### Claude 模型 fallback

`fallback_mode` 取 `off`（預設）、`default` 或 `models`，`fallback_models` 是按順序排列的 upstream
模型 ID 列表。控制台按渠道描述渲染這些欄位。列表會跳過空值、重複項和主模型，最多使用三個 fallback 模型。

- **Claude API、Claude Code 與自定義 Claude 端點：** 傳送 Anthropic 的 `fallbacks` 和對應的 beta 頭。
  `default` 模式交給 Anthropic 決定。
- **OpenRouter：** Claude Messages 傳送 `fallbacks`，Chat Completions 傳送 `models`，由 OpenRouter
  執行模型 fallback。這不改變 `provider.allow_fallbacks`，後者控制的是同一模型的供應商路由。用戶端
  已帶的 `fallbacks` 或 `models` 優先。Responses 請求不會被加上未公開的 fallback 引數。
- **Vercel、Azure、Vertex 與 AWS Bedrock：** GProxy 在同一憑證上用下一個配置的模型重試 Claude
  `refusal`，渠道準備與簽名都重新做。`default` 模式選主模型名稱空間下的 `claude-opus-4-8`。雲廠商
  專用的 ID 請配置該上游接受的完整 ID。

閘道器 fallback 在協議轉換到 Claude 之後同樣生效，但不會重試任意 HTTP 錯誤。串流內容即時下發；已有輸出後，
續接需要帶 prefill 宣告的上游 credit。沒有可兌現的 credit 時絕不重放服務端工具。每次物理嘗試單獨觀測、
按其模型計價；報告零輸出的 refusal 不計費。最終回應保留最後一次嘗試的頂層 usage。

### Claude Code 低優先順序模式

`claudecode` 設定 `low_priority: true` 後，每個 Messages 請求都按 CLI 接受低優先順序提議後的方式傳送
（`anthropic-usage-limit: slow`），5h 視窗用滿也不再把憑證移出輪換；周視窗照常封。是否以低優先順序
服務由 Anthropic 按帳號決定；槽位繁忙時返回 429，GProxy 換下一個憑證。

### 按操作覆蓋 URL

`operation_endpoints` 對某個 Provider 的某個 `(操作, 方言, 传输)` 整體替換方法 URL。它
**不是**一個再拼預設路徑的 base URL；渠道自己的路徑引數由那個方法解析：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/operation-endpoints \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content","dialect":"openai_chat",
       "url":"https://elsewhere.example/v1/chat/completions"}'
```

URL 裡的 `{model}` 會被替換成 upstream 模型名（按一個路徑段做百分號編碼），用於把模型寫在路徑裡的介面：
`https://relay.example/v1beta/models/{model}:generateContent`。

`operation_rules` 是另一半：對某個操作上渠道行為的按 Provider 覆蓋。渠道預設值留在程式碼裡，
不會被拷進每個新建的 Provider。
`POST /admin/api/providers/{id}/routing-defaults/reset` 在一次提交裡把兩者都清掉。

## 一行憑證

| 欄位 | 含義 |
| --- | --- |
| `label` | 可選。登入建立時會從渠道和帳號推導一個。 |
| `authKind` | `api_key`、`oauth` 或 `cookie`——金鑰是怎麼得到的。 |
| `secret` | 該渠道宣告的欄位。從不返回；列表裡只有 `hasSecret`。 |
| `organizationId` / `teamId` / `userId` | 歸屬。恰好一個，或都沒有（共享憑證）。 |
| `connectionProfileId` | 覆蓋 Provider 的出站棧。 |
| `expiresAtMs` | 金鑰該重新整理的時間。 |
| `status` | `active` 或 `dead`。死掉的憑證退出計劃。 |
| `version` | 重新整理寫回時所用的 CAS 守衛。 |

金鑰用主金鑰以 AES-256-GCM 密封，而且**密封繫結到該行自己的 id**，所以一份密文被拷到
另一行上打不開。讀回它是一次單獨的、被審計的呼叫：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/reveal \
  -H "Authorization: Bearer $GPROXY_KEY"
```

沒有主金鑰時金鑰以**明文**存放。這是一種受支援的部署，二進位制在啟動時會說一次。

### 誰能觸達它

一把憑證對某個呼叫方可見，當它**無歸屬**，或者它的歸屬與呼叫方所用 key 的繫結一致——
key 自己的使用者、團隊，或它的有效組織。

是 *key 的*繫結而不是持有人的成員關係，因為一個使用者可以屬於兩個組織而一把 key 只屬於
一個。如果由成員關係決定，一個多組織使用者的每把 key 都會觸達每個組織的訂閱，而且沒有一把
key 能比它的持有人更窄。

可見性只收窄憑證集合，從不擴大它。

## 透過登入獲取憑證

持有帳號而非 key 的渠道提供三種流程中的一種或多種。
`GET /admin/api/channels` 報告每個渠道的 `loginModes`，要一個它不提供的方式會被拒絕。

| 流程 | 步驟 | 適用 |
| --- | --- | --- |
| 授權碼 | start → complete | 帶 PKCE 的瀏覽器跳轉 |
| 裝置碼 | start → 反覆 poll | 在另一臺裝置上敲碼 |
| Cookie 交換 | exchange | 使用者已經有的會話 cookie |

在控制台開啟供應商的憑證標籤，選擇“登入新增憑證”。瀏覽器授權使用完整回撥 URL 完成，
裝置碼流程由頁面自動輪詢。五個 POST 操作位於 `/admin/api/credential-login`，待完成會話繫結
發起使用者和管理範圍。參見[控制台管理](/zh-tw/guides/console/#憑證與上游登入)。

GPROXY 擁有一切不屬於上游的部分。**PKCE verifier** 是本地生成的 32 個隨機位元組，永不外發，
只有它的 S256 摘要會到達授權 URL，因此被截獲的授權碼沒有它也沒用。**CSRF state** 本地
生成、本地比對，不匹配不僅拒絕，還順手銷燬會話，重放因此沒有第二次機會。

待完成的會話存在**共享 cache** 裡，不在本程序。這正是 A 實例開始的登入 B 實例能收尾的
原因——負載均衡後面收尾的通常就是另一個實例——也是被放棄的登入不留痕跡的原因：key 自己
過期，而過期的 key 與從未簽發過的 id 無從區分。

**輪詢是呼叫方的事。** 一次裝置輪詢只走一步就返回，這裡既不 sleep 也不迴圈。應答帶著
該等多久，上游的 `slow_down` 會改寫這個間隔並對之後每次輪詢生效。

一次成功的登入就是一次普通的憑證插入，走管理寫入用的同一個提交原語。金鑰在語句構造
**之前**就密封好，因此渠道與資料庫之間沒有任何東西見過明文，返回的只有憑證 id。

### 重新整理

持有 OAuth 令牌的渠道宣告金鑰何時到期，重新整理返回的是一份**完整替換**——絕不是合併。宿主
以 `version` 上的 CAS 寫回併發出憑證變更通知。

```sh
curl -s -X POST 'http://127.0.0.1:8787/admin/api/credentials/{id}/refresh?force=true' \
  -H "Authorization: Bearer $GPROXY_KEY"
```

上游*確定性*拒絕憑證（`invalid_grant`、已撤銷）會把它標記為死。短暫的傳輸失敗或 5xx
絕不能：它必須以傳輸錯誤的形式浮現，好讓宿主稍後重試。

這條路徑絕不能丟寫入。Claude 每次重新整理都輪換 refresh token，這正是寫回是帶版本守衛的 CAS
而不是盡力而為更新的原因。

## 健康、封禁與計劃

解析直接去掉禁用、已退役和已死的憑證。**被封禁**的憑證——被上游限流的那種——在該
Provider 還有別的可用憑證時被去掉；若**全部**被封禁則保留該 Provider 並排在所有健康
Provider 之後。限流是最後手段，不是故障。

```sh
# 上游怎么说这把凭证的窗口
curl -s http://127.0.0.1:8787/admin/api/credentials/{id}/quota -H "Authorization: Bearer $GPROXY_KEY"
# 现在就问上游
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/quota-probe -H "Authorization: Bearer $GPROXY_KEY"
# 兑换一次重置额度（厂商卖这个的话）
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/quota-reset -H "Authorization: Bearer $GPROXY_KEY"
# 忘掉记录的健康状态
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/health-reset -H "Authorization: Bearer $GPROXY_KEY"
# 覆盖它的运维限额
curl -s http://127.0.0.1:8787/admin/api/credentials/{id}/limits -H "Authorization: Bearer $GPROXY_KEY"
```

`POST …/status` 手動設為 `active` 或 `dead`，帶一個原因。

## 連線配置

一份連線配置就是一次呼叫走的出站棧：代理，以及它呈現的 TLS 與 HTTP/2 身份。選擇順序是
憑證優先，然後 Provider，然後渠道自己的預設，最後實例預設。

內建六種身份預設，可直接存成一份連線配置的 `emulation`：

```sh
curl -s http://127.0.0.1:8787/admin/api/tls-presets -H "Authorization: Bearer $GPROXY_KEY"
```

```text
claude / Claude CLI    codex / Codex CLI       gemini / Gemini CLI
antigravity            kiro / Kiro CLI         copilot / GitHub Copilot CLI
```

只有 `wreq` 傳輸會呈現指紋。模擬 CLI 的渠道返回自己的預設；憑證或 Provider 上的顯式配置
依然勝出。

## 兩個探針

這是本層唯一會離開程序的管理呼叫。

**連通性**透過 scope 指定的那條 client 鏈，問 Cloudflare 的 trace 端點這套部署從外面看是
什麼樣：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/connectivity/test \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"scope":"global"}'
```

```json
{"ok":true,"latencyMs":803,"ip":"223.166.167.192","colo":"LAX","error":null}
```

scope 可以是 `global`、`{"scope":"provider","provider_id":"…"}`、
`{"scope":"credential","credential_id":"…"}`——它的呼叫所用的那條傳輸——或
`{"scope":"proxy","url":"…"}`，用來測一個還沒配置到任何地方的代理。
**網路失敗是 `ok: false` 加一個原因，不是錯誤**："上游不可達"正是問題要的答案。

**模型測試與發現**像呼叫方的請求一樣走完引擎：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/models/test \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","model":"gpt-4o-mini"}'

curl -s -X POST http://127.0.0.1:8787/admin/api/models/discover \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…"}'
```

它們**花的是真憑證、消耗真上游配額、對該憑證適用的任何預算真結算，並寫一行用量**。
沒有 dry run：一個沒有真正呼叫上游的測試什麼也沒測到。

發現用 Provider 自己的方言提問，因此名字就是上游的。每一個回來時都帶著"這個 Provider
是否已有這一行"和"內建目錄能不能給它定價"；
`POST /admin/api/models/discover/apply` 插入你點名的那些，已有的跳過。

## 搬運一份配置

```sh
gproxy export --out config.json --include-secrets
gproxy import --in  config.json --mode merge --source-master-key '…'
```

會走的是一套部署*本身*的配置：連線配置、Provider、憑證、模型目錄、路由、操作覆蓋、
改寫規則、配額、價格和 settings 行。**身份不走**——使用者、key、組織、團隊、權限和訂閱屬於
產品層——用量和 capture 也不走，因為複製它們等於偽造目的端從未有過的歷史。主金鑰規則見
[配置](/zh-tw/reference/configuration/#搬運一份配置)。

## 請求頭白名單

在“設定 → 網路”配置全域性請求頭白名單，每個供應商透過 `config.allowed_headers`
補充允許項。最終集合為 **全域性白名單 ∪ 供應商白名單 ∪ 渠道預設頭**，並保留
`content-type`。未配置或空列表不增加允許項，其他用戶端頭預設丟棄。原始鑑權頭和
逐跳頭即使列入名單也會移除；渠道注入的鑑權和靜態 `headers` 配置獨立於此規則。
配置熱更新後生效，不會把合併結果寫回供應商儲存的列表。

### GLM 與 MiniMax

`glm` 使用按量 API，`glmcode` 使用 GLM Coding Plan，兩者都填 `api_key`。預設地址是國內 `https://open.bigmodel.cn`；國際帳號將 `baseUrl` 設為 `https://api.z.ai`。`glmcode` 支援官方 Chat、Claude Messages 和 Responses 訂閱入口，不會在額度耗盡後切換到普通 API。

兩個渠道的模型列表均沿用 ZCode 官方動態目錄：查詢 `/api/v1/client/configs`，下載返回的 `builtin_provider_config_json`，再選擇對應地區的普通 API 或 Coding Plan 模板。沒有內建固定模型名單，也不在下載失敗時回退到舊名單。目錄是官方產品列表，不代表該 Key 對每個模型都有權限。

`minimax` 預設地址為 `https://api.minimax.io`；國內帳號按其平台文件填寫國內 API origin。文字支援 OpenAI Chat 和 Claude Messages，Subscription Key（Coding Plan／Token Plan）與按量 API Key 都填入 `api_key`。模型列表和詳情使用官方 `/v1/models` 與 `/v1/models/{id}`。兩類 Key 可以放在不同供應商中，分別配置文字與影片模型路由。

額度查詢：`glmcode` 讀取官方 `/api/monitor/usage/quota/limit`，顯示服務端提供的用量百分比、獨立視窗和重置時間；普通 `glm` 尚未接入帳戶餘額查詢。MiniMax 按官方 CLI 的 Key 型別判斷讀取 `/v1/token_plan/remains` 或 `/account/query_balance`。當前這些查詢結果用於展示，是否耗盡仍由上游請求結果判定。

MiniMax 影片接入 **H3 V2**（`MiniMax-H3`、`MiniMax-H3-Max`），需要按量 API Key。使用 GPROXY 的 OpenAI 影片擴充套件 JSON：

```json
{
  "model": "MiniMax-H3",
  "prompt": "一只猫在花园里奔跑",
  "duration": 5,
  "resolution": "2K",
  "aspect_ratio": "16:9"
}
```

向 `/v1/videos` 提交後，使用返回的 `id` 查詢 `/v1/videos/{id}`，完成後從 `/v1/videos/{id}/content` 下載。支援 `frame_images` 首尾幀及 `input_references` 影象、影片、音訊參考，也可傳 MiniMax 原生 `content`。不支援 Sora multipart、`seconds` 或 `size` 引數；舊版 Hailuo 2.x V1 介面不在本渠道覆蓋範圍內。

列表使用 `page_num`／`page_size`（`limit` 會對映成 `page_size`），不支援遊標 `after`／`order`。刪除介面保留上游的 `action`：`cancelled` 表示取消待執行任務，`deleted` 才表示刪除任務記錄。渠道不會自動輪詢，也不會儲存影片檔案。

介面依據：[ZCode 動態目錄實現](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/packages/provider-node/src/zcode-builtin-download.ts)、[MiniMax 官方 CLI](https://github.com/MiniMax-AI/cli)、[GLM Coding Plan](https://docs.bigmodel.cn/cn/coding-plan/tool/others)、[MiniMax 文字](https://platform.minimax.io/docs/api-reference/text-anthropic-api)、[MiniMax H3 V2](https://platform.minimax.io/docs/api-reference/video-generation-v2-create)。
