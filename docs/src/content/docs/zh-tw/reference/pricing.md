---
title: "價格與分層"
description: GPROXY v4 怎麼給一次交換定價：規則選擇、費率行及其條件、上下文與服務檔位階梯，以及完全沒有價格時會怎樣。
---

計價在結算時回答一個問題：**這次交換花了多少**——給定 Provider、上游模型，以及渠道提取出
的歸一化用量。

它發生在**引擎內部**、在結算時，因此預算和觀察者看到的是同一個數。v3 由引擎把用量交上去、
再由應用層算一遍，於是產生了兩套時間語義、兩套模型匹配，以及兩個"沒有價格怎麼辦"的答案。
v4 每樣只有一個。

三張表，一種形狀：一行 `price_rules` 選中一個模型，它的 `price_rates` 行給每個指標定價，
它的 `price_tiers` 行按提示長度和服務檔位調整 token 階梯。

## 規則選擇

一條規則適用，當

- 它的 provider 是這次交換的 provider，**或未設**（一條全域性規則）；
- 它的 `modelPattern` glob 匹配**上游**模型名；
- 它的 `operation` 是這次請求的，**或未設**（覆蓋全部操作）。

**Provider 規則先於全域性規則。** 同一個作用域內 `(priority, id)` 最小者勝。

| 欄位 | 含義 |
| --- | --- |
| `providerId` | `null` 是全域性規則 |
| `modelPattern` | 對上游模型名的 `*` / `?` glob |
| `operation` | `null` 覆蓋該模型的每個操作 |
| `priority` | 越小越先；id 破平 |
| `currency` | 固定為 `USD`，所有價格和結算金額統一使用美元 |
| `enabled` | |

### 一條規則都沒有

這次交換是**未定價**的：不花錢，照樣結算，而且它的用量帶著

```json
{"dimensions": {"unpriced": "true"}}
```

這是一個訊號，不是一個拒絕。運維者想知道有個模型在被白嫖；他們不想因為還沒人填價格就
被拒掉一個請求。

## 費率行

一個費率行是某個指標的基礎價。

| 欄位 | 含義 |
| --- | --- |
| `metric` | 一個內建鍵，或渠道產出的任意自定義鍵 |
| `unit` | `token`、`count`、`second` 或 `character` |
| `unitQuantity` | 正的分母：1 000 000 個 token、1 張圖、60 秒 |
| `value` | `unitQuantity` 個單位的非負價格，幣種取自父規則 |
| `conditions` | `null` 是兜底行；否則是一個非空的"維度名 → 標量"物件，**全部**條件都要匹配 |
| `priority` | 同一指標的多行中越小越先 |

對一個指標，帶條件的行按 `(priority, id)` 順序嘗試，第一個條件全部匹配結算維度的勝出；
否則兜底行生效。**被選中的條件費率*替換*基礎費率**，不是與它複合。

```json
[{"metric":"input_tokens",  "unit":"token","unitQuantity":"1000000","value":"0.40"},
 {"metric":"output_tokens", "unit":"token","unitQuantity":"1000000","value":"1.60"},
 {"metric":"image_outputs", "unit":"count","unitQuantity":"1","value":"0.04",
  "conditions":{"quality":"hd","size":"1024x1024"},"priority":0}]
```

### 阻止重複計費的那條規則

**費率必須消費互不相交的量。** 命中快取的 input 要從普通 input 里扣掉；已經透過聚合項
計費的 reasoning 或媒體 token 子集不能再收一次。按圖計費和按 token 計費是*互斥*的基準，
除非廠商真的兩樣都收。

一個通用的 `tool_calls` 行和一個具體的 `web_searches` 行不能給同一次呼叫都計費。而用戶端
*宣告*了一個工具，並不構成一次可計費的服務端執行。

## 指標

**token**——習慣上按每百萬計：

```text
input_tokens          output_tokens           cached_input_tokens
cache_creation_5m_tokens  cache_creation_30m_tokens  cache_creation_1h_tokens
reasoning_tokens      image_input_tokens      image_output_tokens
audio_input_tokens    cached_audio_input_tokens  audio_output_tokens
video_input_tokens    video_tokens
```

**計數、秒與字元：**

```text
image_outputs   video_outputs   audio_seconds   video_seconds   audio_characters
search_units    web_searches    web_fetches     file_searches
code_interpreter_sessions       tool_calls      requests
```

不在這張表裡的指標，只要有費率行點名它，照樣被定價。這張表說的是內建鍵有哪些，不是一個
過濾器。

`requests` 是每請求費：每個**被定價的交換**計一個請求。

## 分層

一行 `price_tiers` **只**覆蓋 token 價格，而且一律按每百萬。

| 欄位 | 含義 |
| --- | --- |
| `serviceTier` | `null` 使它成為一個**上下文**檔位；否則是 `standard`、`priority`、`flex`、`batch`… |
| `minPromptTokens` | 非負的含端點閾值；預設 0 |
| `multiplier` | 服務檔位用：把繼承來的價格乘以多少。僅上下文的行不設它 |
| `*_per_million` | 某一類 token 的顯式價格 |

顯式列有 `input`、`output`、`cache_read`、`cache_creation_5m` / `30m` / `1h`、
`reasoning`、`image_input`、`image_output`、`audio_*` 和 `video_*`。

**提示長度算的是 input 加快取讀加快取寫，各計一次。**

### 按 token 類的複合

1. **基礎費率**——該指標的 `price_rates` 行，帶條件的先試。
2. **上下文件位**——在**沒有**服務檔位的行裡，取已達到的最大 `minPromptTokens`。它的顯式
   價格替換掉它所設的那些類的基礎價。
3. **服務檔位**——在點名了*實際*服務檔位的行裡，取已達到的最高閾值。那裡的顯式價格
   **直接勝出**；否則把上下文調整後的基礎價乘以該行的 `multiplier`（預設 1）。

閾值相同時按 `(priority, id)` 較小者破平。`null` 表示繼承；0 表示**明確免費**，不是缺失。

乘數從不作用於工具或媒體的*計數*——只作用於 token 價格。

:::caution[顯式檔位價會替換掉整條階梯]
基礎 input 價為 1、上下文臺階 `≥ 200 000 → 2`、提示 300 000 個 token 時：

| `batch` 行 | 實際 input 費率 |
| --- | --- |
| `{"serviceTier":"batch","multiplier":"0.5"}` | `2 × 0.5 = 1` |
| `{"serviceTier":"batch","inputPerMillion":"0.5"}` | `0.5`——200k 那級丟了 |
| `{"serviceTier":"batch","minPromptTokens":200000,"inputPerMillion":"1"}` | `1` |

要麼在它必須覆蓋的每一級都重複寫一遍顯式檔位價，要麼用乘數。
:::

### 兩個特例

- **reasoning 是 output 的子集。** 存在 reasoning 價格時，reasoning token 從
  `output_tokens` 里扣出來單獨計；不存在時它們留在 output 裡，只被計一次。
- **沒有價格的快取讀繼承 input 價。** 沒有價格的*其他*每一類都免費。這個不對稱是刻意的：
  快取讀毫無疑義就是一個 input token，而缺失的音訊或影片價格是運維者應該看到為 0 的配置缺口。

### 要求的檔位 vs 實際提供的

被收費的是回應**實際報告**的那個檔位，不是請求要求的那個。一個被廠商降級的 `priority`
請求，按降級後那一行結算。

## 其餘一切

不是 token 類的每個指標都按

```text
amount × value / unitQuantity

```

計費，而且**從不被服務檔位乘**。

## 編輯價格

```sh
curl -s http://127.0.0.1:8787/admin/api/price-rules  -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:8787/admin/api/price-rates  -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:8787/admin/api/price-tiers  -H "Authorization: Bearer $GPROXY_KEY"
```

每個家族都有通常的五條路由外加一個批次，而一次批次無論點名多少行都是一個 revision 提交。

內建目錄能把本次釋出知道的模型填進去：

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/default-model-catalog/apply-prices \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","modelIds":["gpt-4o-mini"],"overwrite":false}'
```

不帶 provider 時使用目錄自己的 glob 與優先順序，因此一條規則給這個模型在任何地方定價；
帶 provider 時字面名字成為 pattern，優先順序為 0。**`overwrite: false` 正是讓重複應用安全的
東西**：運維者改過的規則保留它的改動，並被報告為跳過。

一行壞的計價資料在裝配時被丟掉並打 warn，**不會**阻塞快照。一個打錯的小數不該拖垮一個
本來還在正常服務流量的實例。

## 預算花的是這個結果

一個預算就是一行 metric 為 `cost`、unit 為 USD 的 `quotas`。准入把呼叫方的鏈
`[api_key?, user, team?, org?]` 交給引擎，而鏈上**任何** owner 的**每一個**
啟用預算都適用。

費用在上游呼叫結束後結算，在途請求和併發呼叫可能使預算超出上限。預算歸屬與檢查方式見[權限、限流與費用預算](/zh-tw/guides/permissions/)。

## Token 估算

當一個可計費的回應完全沒帶用量時，引擎自己數 token，並把記錄標為 `estimated = true`。
計數用的是本地詞表：GPT 系列用 tiktoken 編碼，其他用該模型目錄行點名的詞表，再沒有就用
內建的 DeepSeek。

```sh
curl -s http://127.0.0.1:8787/admin/api/tokenizer-vocabs -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X POST http://127.0.0.1:8787/admin/api/tokenizer-vocabs \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"repo":"…","filename":"tokenizer.json","modelId":"…","setAsDefault":true}'
```

一次抓取經配置好的檔案儲存下載檔案，然後插入那一行並把模型與預設值指過去——**一行加兩個
指標在一個 revision 提交裡**。位元組先落地、行後落庫：一行指向從未被寫入的物件會讓此後每次
過載都失敗，而一個沒有行的物件只是浪費空間。

沒有檔案儲存時整個家族回答"不支援"：沒有地方放這些位元組。
