---
title: 提示快取
description: "透過快取斷點規則或魔法字串配置提示快取，檢視用量與計價。"
---

提示快取由上游提供，命中情況取決於模型、提示字首和有效期。GPROXY 可以透過快取斷點規則或提示中的魔法字串新增標記；新增標記不保證命中，也不能讓不支援快取的上游獲得快取能力。

## 在控制台新增快取斷點

在供應商的重寫規則中新增“快取斷點”，選擇上游協議和位置。位置包括全域性、系統和訊息；Claude 還支援工具。區域性位置可填寫索引，正數從 1 開始，負數從末尾計數，留空選擇最後一個可用位置。

Claude 可選擇預設、5 分鐘或 1 小時 TTL；OpenAI 可選擇預設或 30 分鐘。實際生效方式取決於上游。規則中的協議是轉換後的上游協議，Gemini 不提供這個規則型別。

透過 API 配置時使用 `action: "cache_breakpoint"`，`replacement` 是配置物件的 JSON 字串，例如：

```json
{"dialect":"claude","target":"message","index":-1,"ttl":"5m"}
```

這條規則作用於請求正文，與其他重寫規則按配置順序執行。用戶端能自行提交快取標記時，也可以直接使用上游支援的欄位。

## 使用魔法字串

無法直接設定快取欄位的用戶端，可以在提示文字中放置以下標記，並啟用供應商對應的 magic cache 選項。

### 三個字串

```text
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF
GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY
```

| 字串 | 在 Claude 目標上 | 在 OpenAI 目標上 |
| --- | --- | --- |
| …`7D9ASD…` | `cache_control`，用 API 的預設 TTL | 一個顯式斷點 |
| …`49VA1S…` | `cache_control`，`"ttl": "5m"` | 一個顯式斷點 |
| …`1FAS5G…` | `cache_control`，`"ttl": "1h"` | 一個顯式斷點 |

三個在 OpenAI 上產生同一個斷點，因為 OpenAI 沒有按塊的 TTL。

它們是**凍結的**：它們是用戶端到代理之間協議的一部分，不隨版本變化。

## 最要緊的那條規則

**無論快取是否啟用，token 總是被剝掉。** 一個洩漏到別人提示裡的標記，任何時候都不是
想要的東西。

開關決定的只是要不要在 token 所在的位置放一個*斷點*。magic cache 關閉時，token 被取走，
別的什麼也不動：用戶端自己的 `cache_control` 或 `prompt_cache_breakpoint` 原樣保留，
而不含 token 的 body 逐位元組轉發，既不解析也不重新序列化。

只剝不放的那一趟也正是 `claudeweb` 所做的，因為瀏覽器會話根本沒有快取控制。

## 啟用它

兩個 Provider `config` 開關，每族一個：

```json
{ "config": { "enable_claude_magic_cache": true,
              "enable_openai_magic_cache": true } }
```

| 開關 | 提供它的渠道 |
| --- | --- |
| `enable_claude_magic_cache` | `aws_bedrock`、`azure`、`claudeapi`、`claudecode`、`custom`、`opencodego`、`opencodezen`、`openrouter` |
| `enable_openai_magic_cache` | `aws_bedrock`、`azure`、`openai`、`codex`、`custom`、`opencodego`、`opencodezen`、`openrouter` |

開關是對著**目標方言**讀的，不是用戶端的。一個被路由到 Claude Provider 的 OpenAI 形狀
請求會先轉成 Claude Messages，因此起作用的是 Claude 開關，寫下的標記是 `cache_control`。
用戶端收到的仍然是它自己的格式。

## 標記落在哪

### Claude Messages

body 先被規範化：字串 content 變成塊陣列，空文字塊被丟棄，落在被丟棄塊上的
`cache_control` 移到最近的可快取塊上——這正是讓一個單獨成行的 token 也能工作的原因。
thinking 塊只在 assistant 輪次保留。

然後 `system` 在 `messages` **之前**被遍歷，因此預算按提示順序花掉，而不是按 JSON 鍵序。
每個帶 token 的文字塊都被標記：

```json
{ "cache_control": { "type": "ephemeral", "ttl": "1h" } }
```

已經帶 `cache_control` 的塊保持不動。

### OpenAI Chat Completions

除 `function` 角色外的每條訊息都可以被標記。純字串 `content` 會被拆成一個被標記的
`text` part，因為標記需要一個落腳處；陣列 content 則就地標記它的 part。

```json
{ "type": "text", "text": "…", "prompt_cache_breakpoint": { "mode": "explicit" } }
```

### OpenAI Responses

三個位置，依次是：

1. **`instructions`。** 那裡的 token 被剝掉，並在 `input` 前面插入一條被標記的 developer
   訊息——`instructions` 是字串，自己承載不了標記。
2. **`prompt.variables`**，像 Chat 的 part 一樣標記。
3. **`input`**，無論它是裸字串（變成一條被標記的 user 訊息）還是一組 item，後者的
   `content` 與 `output` 用 `input_text` / `output_text` 型別標記。

### Gemini

沒有斷點。token 被剝掉，別的什麼都不發生。

## 四個的上限

**最多四個斷點離開代理，含用戶端自己的。** 預算是數 body 裡已有的標記算出來的，所以一個
自己放了三個的用戶端還剩一個名額。

超出上限的 token 仍然被剝掉——只是不變成標記。這和別處是同一條規則：這些字串永遠不會
到達上游。

## 用量與計價

快取 token 有自己的指標，結算時按你的費率行定價：

| 上游報了什麼 | 指標 |
| --- | --- |
| 快取讀 | `cached_input_tokens` |
| 5 分鐘快取寫 | `cache_creation_5m_tokens` |
| 30 分鐘快取寫 | `cache_creation_30m_tokens` |
| 1 小時快取寫 | `cache_creation_1h_tokens` |

`cached_input_tokens` 以 `input_tokens` 為上限：未命中的餘量按 input 價計，命中部分按
快取讀價計，而快取讀**沒有價時繼承 input 價**。沒有自己費率行的快取寫按 0 結算。見
[價格與分層](/zh-tw/reference/pricing/)。

單條記錄上的 token 數是可空的——上游沒報某個欄位，不等於它測到了 0——而每個合計都是普通
數值，因為"什麼都沒報"的求和確實是 0。

## 保住命中

改寫規則跑在轉換**之後**、渠道之前，所以一條改提示文字的規則可以把一次本該命中的快取變成
未命中。把穩定內容放在最前、標記放在它之後、每請求都變的內容全部放在邊界之後——並且檢查
沒有哪條 `paths` 選得過寬的改寫規則在改這段字首。

見[改寫規則與操作覆蓋](/zh-tw/guides/rules/)。
