---
title: "模型與路由"
description: 用戶端的模型名如何解析到一個 Provider 和一把憑證：四種形式、路由與成員、namespace 與模型目錄。
---

用戶端的模型名很少就是上游模型 id。v4 在**進引擎之前**解析它，因此引擎只見已選定的目標。

```text
请求里的 model
  → 第一条命中的形式：路由名 · 渠道/模型 · Provider 名/模型
  → 候选按渠道、允许的 Provider、允许的凭证收窄
  → 按 (tier, 健康度, 权重倒序, 稳定 id) 排序
  → 首段按路由策略均衡
  → 每次尝试得到一个 (Provider, 凭证, 上游模型)
```

直接使用路由名稱作為用戶端模型名。需要修改請求引數時，使用[重寫規則](/zh-tw/guides/rules/)。

## 四種形式

按第一條命中的規則解析。

| 名字 | 解析為 | 嘗試預算 |
| --- | --- | --- |
| 沒有模型 | 全部啟用的 Provider，無上游模型 | `settings.max_attempts` |
| **模型路由名稱** | 該路由啟用的成員 | 路由自己的 |
| `渠道/模型` | 該渠道的 Provider，優先目錄裡列了該模型的 | `settings.max_attempts` |
| `Provider 名/模型` | 那一個 Provider | `settings.max_attempts` |
| 其他 | `404 unknown_model` | — |

路由名**精確匹配且先於**字首形式。管理介面會拒絕首段與已註冊渠道或 Provider 字首衝突的名稱。

字首形式內部，**渠道 id 勝過同名 Provider**。渠道 id 由構建固定、改不掉；Provider 名隨時
可以改。反過來更糟：把某個 Provider 命名為 `codex`，全部 `codex/*` 流量就再也到不了
`codex` 渠道，而且沒有任何繞開的辦法。

## 路由

路由名稱就是用戶端請求的模型名，下含供應商／上游模型成員、均衡策略和嘗試預算。
建立 `main` 路由後，用戶端直接請求 `model: "main"`。

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/routes \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"name":"main","strategy":"round_robin","maxAttempts":6}'
```

```json
{"id":"33a88261f571347c7f0408c3bd2e2164","name":"main","strategy":"round_robin",
 "maxAttempts":6,"enabled":true}
```

| 欄位 | 含義 |
| --- | --- |
| `name` | 全域性唯一，用戶端在 `model` 中直接填寫此名稱。 |
| `sessionAffinity` | 會話優先複用成功的供應商／模型目標，預設關閉；內部憑證選擇仍由供應商負責。 |
| `strategy` | `round_robin`、`weighted` 或 `failover`。 |
| `maxAttempts` | 含首次呼叫在內的總嘗試預算。執行時以 `settings.maxAttempts`（預設 6）為硬上限。 |

## 成員

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/route-members \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"routeId":"…","providerId":"…","upstreamModel":"gpt-4o-mini",
       "tier":0,"weight":100}'
```

| 欄位 | 含義 |
| --- | --- |
| `providerId`、`upstreamModel` | 這個成員把流量發到哪。模型名是一個顯式字串，不是目錄外來鍵。 |
| `tier` | 越小越優先。tier 0 耗盡之前 tier 1 拿不到任何流量。 |
| `weight` | 正數，預設 100。層內分流，同時決定故障轉移候選的順序。 |
| `enabled` | 被禁用的成員退出計劃。 |

成員不指定憑證。選中的 Provider 內部由哪把憑證承接是引擎另做的決定，而且它會在計劃移動到
下一個成員之前，先在該 Provider 的憑證之間做轉移。

### 順序怎麼定

候選按 `(tier, 健康度, 权重倒序, 稳定 id)` 排序。

**tier 是硬偏好。** 只有首段——與第一個候選 tier *和*健康度都相同的那一串——參與均衡，
然後才按策略處理：

| 策略 | 對首段的作用 |
| --- | --- |
| `round_robin` | 用一個按路由的計數器輪轉它 |
| `weighted` | 把平滑加權選中的提到隊首 |
| `failover` | 原樣保留：排序結果*就是*答案 |

輪轉是計數器而不是隨機抽取，因此 wasm 與原生行為一致，一次序列可復現。

禁用、已退役和已死的憑證直接去掉。被封禁的憑證在其 Provider 還有別的可用憑證時去掉；
若**全部**被封禁則保留該 Provider 並排在所有健康 Provider 之後。限流是最後手段，不是故障。

解析成功但無處可發，與"名字不認識"是兩回事——那是配置問題，不是未知模型。

## 路由名稱

直接在路由上建立和修改用戶端使用的模型名，不需要額外的公開名稱對映。

### namespace

帶 `/` 的名字在執行期派生出一個 namespace：建立 `acme/fast` 路由 就讓 `acme` 成為一個掛載點，
而 `/acme/v1/chat/completions` 配 `{"model":"fast"}` 解析的是 `acme/fast`。

```sh
curl -s http://127.0.0.1:8787/acme/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"hi"}]}'
```

namespace 是一個**名字索引**，不是被儲存的分組，也不是歸屬範圍。它不建立任何東西，也不
擁有任何東西。

### 被保留的第一段

為了避免 namespace 與字首路由歧義，首段是已註冊渠道 id 或現有 Provider 名的路由名會在寫入時被拒絕：

```json
{"error":{"code":"invalid_request","message":"invalid request: `codex/` is reserved:
 a first segment naming a channel or a provider already means `channel/model` or
 `provider/model` narrowing, so `codex/fast` could never reach its route"}}
```

其他都行。`coding/fast` 是個完全好用的公開名。

## 模型目錄

兩張表，而且路由都不需要它們。

**`provider_models`** 記錄某個 Provider 承接哪些上游名字：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/provider-models \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","upstreamName":"gpt-4o-mini"}'
```

```json
{"id":"f32df0bb6378c03e70f7c3aa6315a3b8","providerId":"5a45fd807be0…",
 "upstreamName":"gpt-4o-mini","modelId":null,"metadata":{},"enabled":true}
```

`渠道/模型` 形式在該渠道的多個 Provider 中做選擇時會優先它，模型發現也寫在這裡。
**`models`** 是一行 `provider_models` 可以指向的全域性目錄：一個名字、它的後設資料，以及
token 估算該用的詞表。

路由可以匹配兩張表裡都沒有的名字。路由成員把上游模型寫成一個普通字串，所以目錄行是
文件與定價材料，不是前置條件。

### 從上游填充

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/models/discover \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…"}'
```

發現用 Provider **自己的方言**提問，因此什麼都不轉換，名字就是上游的。每個答案帶著這個
Provider 是否已有該行、以及內建目錄能不能給它定價。
`POST /admin/api/models/discover/apply` 插入你點名的那些，已有的跳過，所以發現應用兩次和
一次結果相同。

它和 `POST /admin/api/models/test` 都花真憑證、寫真用量行。見
[兩個探針](/zh-tw/guides/providers/#兩個探針)。

內建目錄——本次釋出知道的模型名、上下文視窗與預設價格——是生成那份資產時的快照，不是一份
實時目錄：

```sh
curl -s http://127.0.0.1:8787/admin/api/default-model-catalog -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/default-model-catalog/apply-prices \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","modelIds":["gpt-4o-mini"],"overwrite":false}'
```

`overwrite: false` 正是讓重複應用安全的東西：運維者改過的規則保留它的改動，並被報告為跳過。

## 呼叫方看到什麼

`GET /v1/models` 返回已配置的模型清單，包括路由名和
`Provider 名/模型`，只列出呼叫方有權使用的名字，不會輪流返回某一家上游的目錄。
新增或匯入供應商模型後，它會出現在這份清單中，字首使用配置的供應商路由名。
`渠道/模型` 仍可用於呼叫，但不會再自動新增為另一條記錄。

兩種呼叫方式等價：API 地址用 `/供应商A/v1`、模型填 `gpt-5`；
或者地址用 `/v1`、模型填 `供应商A/gpt-5`。
`GET /供应商A/v1/models` 只向供應商 A 查詢，保留上游模型原名。

使用者面的清單還會顯示沒有權限的名字，並用 `permitted` 標註：

```sh
curl -s http://127.0.0.1:8787/portal/api/models -H "Authorization: Bearer $GPROXY_KEY"
```

```json
[{"name":"provider-A/gpt-4o-mini","providerCount":1,"channelIds":["custom"],"permitted":true},
 {"name":"fast","providerCount":1,"channelIds":["custom"],"permitted":true}]
```

呼叫方規則觸達不到的名字仍然在清單裡，只是 `permitted: false`。v3 會丟掉這樣的行，v4 不
——因為一份會靜默省略的清單會讓"這個模型 404 了"和"你沒有權限用這個模型"變成同一個觀察，
而且沒什麼要保護的：公開名本來就是運維者對外發布的實例配置。

被扣下的是名字背後的 Provider id：答案只報一個數量和一個渠道，說明一個名字有多冗餘，
卻不點破機器。`Provider 名/模型` 使用供應商的顯示名稱；供應商改名後，對應的模型字首和 API 地址也隨之改變。

### 全域性模型與預設後設資料

Console 的“模型”（`/console/model-catalog`）展示內建模型、上下文和輸出上限、輸入輸出模態、支援引數、參考價格與分段價格。可搜尋模型，儲存本地後設資料覆蓋值，或新增自己的模型。移除本地條目後，內建模型仍保留在列表中。

預設模型表使用 `claude-sonnet-4` 這樣的裸模型名，去掉 `anthropic/` 等來源字首；來源 URL 和原始報價仍保留。帶字首的上游名稱仍可按末段模型名匹配。預設價格選擇最長的匹配模型片段，供應商匯入價格時按其實際上游名稱儲存規則。價格繼續在全域性模型和供應商模型的現有彈窗中編輯。

渠道拉取時，優先按完整模型 ID 匹配，再嘗試唯一的末段名稱；有歧義則不補全。合併順序為內建預設值 → 上游非空欄位 → 本地覆蓋值。Console 匯入已有渠道模型時只補缺失欄位，已有設定不被覆蓋。後設資料是匯入時複製的快照，編輯總表不會自動修改已經匯入的渠道模型。

預設價格是 OpenRouter 報價快照，不代表所有渠道的實際合同。應用預設價格會生成全域性計費規則，已有同模式規則不覆蓋；價格編輯器支援費率、上下文分段及服務等級。渠道專屬規則優先於全域性規則。

內建資料透過 `node scripts/update-openrouter-model-catalog.mjs` 更新；可用 `--input response.json` 離線生成。公開模型介面無需 Key；可選憑證只從 `OPENROUTER_API_KEY` 環境變數讀取。生成器保留原始 `source_pricing` 作為核查材料，只把能確認單位的欄位寫入計費結構。單位換算後的計費價格自動舍入到 9 位小數，採用就近舍入、中點取偶；匯入預設價格時也使用相同規則。缺失或動態價格不應當解釋為免費。額外官網價格應先確認模型版本、地區、單位和閾值，再加入，不以猜測補齊。
