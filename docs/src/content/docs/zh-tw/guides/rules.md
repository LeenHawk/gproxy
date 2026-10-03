---
title: "重寫規則與協議覆蓋"
description: "設定提示詞、快取斷點、JSON 與文字重寫，以及上游協議和端點。"
---


在供應商的重寫規則頁面新增規則，選擇型別並填寫內容。新建供應商會自動建立並繫結同名預設規則集，一般不需要手動管理繫結；多個供應商需要共用規則時，再使用高階繫結設定。

## 規則型別

| 介面型別 | 用途 | API `action` |
| --- | --- | --- |
| 系統提示詞 | 在現有系統指令前或後新增文字 | `system_text` |
| 快取斷點 | 按協議為全域性、系統、訊息或工具新增快取標記 | `cache_breakpoint` |
| JSON 改寫 | 設定、刪除欄位，或合併 JSON 物件 | `set`、`delete`、`merge` |
| 文字轉換 | 對正文、請求頭或查詢引數做正則替換 | `replace` |
| 請求頭 | 設定值，或合併逗號分隔的值 | `header_set`、`header_merge` |

系統提示詞支援 Claude、OpenAI Chat、Responses（含 WebSocket）和 Gemini。快取斷點支援 Claude 與 OpenAI 協議；具體位置和有效期見[提示快取](/zh-tw/guides/claude-caching/)。

## 高階 JSON 編輯

在規則集內新建或編輯規則時，切換到“高階 JSON”即可直接貼上一個 v4 規則物件。供應商的重寫規則頁面也提供此入口。表單和 JSON 可以相互切換，並支援格式化；JSON 有誤時會保留輸入並提示，儲存時還會校驗規則是否可執行。

例如，將正文中的 `"type":"function"}` 替換為 `"type":"function","strict":false}`：

```json
{
  "action": "replace",
  "phase": "request",
  "target": "body",
  "pattern": "\"type\":\"function\"\\}",
  "replacement": "\"type\":\"function\",\"strict\":false}"
}
```

這裡使用 v4 的 `action`、`pattern`、`replacement` 欄位。規則集、ID 和順序由頁面管理，不填入 JSON；可選的過濾條件及 `enabled` 可以一併填寫。省略過濾條件表示不限制匹配，省略 `enabled` 表示啟用。一次編輯一條規則，不接受規則陣列。

## 執行位置與順序

請求先轉換為上游協議，再執行請求重寫，然後由渠道傳送。回應先執行回應重寫，再轉換為用戶端協議。因此，規則中的欄位路徑必須使用**上游格式**。

例如，OpenAI Chat 請求轉到 Claude 供應商後，規則應使用 Claude 的 `system` 和 `messages` 欄位。

規則集繫結和集內規則分別按 `sortOrder`、ID 排序，只執行啟用項。後一條規則會看到前一條的結果。系統提示詞、快取斷點和其他規則也遵循這個順序。

## 欄位與正則

JSON 改寫需要填寫路徑，例如 `temperature` 或 `messages.*.content`。路徑中的 `*` 表示所有元素。`set` 的值必須是合法 JSON，`merge` 的值必須是 JSON 物件；`delete` 不需要值。

文字轉換使用 Rust 正則語法，支援 `(?i)`、`(?s)` 和替換中的 `$1`、`${name}`。填寫正文路徑時只替換匹配的 JSON 字串值；不填路徑時對整段正文文字替換。修改 JSON 文字時應避免破壞語法。

請求頭和查詢引數規則需要 `targetName`。`header_set` 替換請求頭值；`header_merge` 合併逗號分隔的值並去重。不要把上游金鑰寫進可共享的規則。

## 過濾條件

| 條件 | 匹配內容 |
| --- | --- |
| 操作與協議 | 轉換後在供應商上執行的操作 |
| 模型 | 上游模型或用戶端請求的模型名，支援 `*`、`?` |
| 請求頭 | 入站請求頭的正則匹配 |
| 流事件 | SSE 事件名或 JSON `type`，用於正文規則 |

多個條件同時滿足時規則才生效。省略的條件不限制匹配。為某個用戶端新增相容規則時，應配置請求頭過濾，避免影響其他用戶端。

模型篩選的候選只來自供應商已新增的模型和變體。供應商頁面僅顯示當前供應商的模型；獨立規則集頁面顯示關聯供應商的模型並集。未關聯供應商時沒有候選，仍可手動填寫模型名或 `gpt-*` 等匹配模式。候選列表不會請求上游模型目錄。

## 串流回應

正文重寫以完整事件為單位執行，例如 SSE 事件或 JSON 陣列元素，不以網路資料塊為單位。單個事件超過 `settings.maxStreamEventBytes` 時會報錯。可以用事件過濾器只修改特定型別的事件。

## 透過 API 管理

下面建立一條正則替換規則，把指定路徑中的 `widget` 替換為 `gadget`。將規則集 ID 替換為你的實際值。

```sh
curl -sS -X POST http://127.0.0.1:8787/admin/api/rules \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"ruleSetId":"your-rule-set-id","action":"replace",
       "phase":"request","target":"body","paths":["messages.*.content"],
       "pattern":"(?i)\\bwidget\\b","replacement":"gadget",
       "filterOperationKeys":[{"operation":"generate_content","dialect":"openai_chat"}]}'
```

`PUT /admin/api/rule-sets/{id}/rules` 可一次替換整組規則。系統提示詞與快取斷點的 `replacement` 是 JSON 編碼的配置字串，控制台會生成它，不需要手工拼接。

## 相容預設

`GET /admin/api/rule-presets` 返回內建的 OpenCode、the agent-mono、Aider、Cline、Continue 和 Cursor 相容預設。規則數量與內容以當前介面返回為準。

`POST /admin/api/rule-sets/{id}/rule-presets/{preset}` 會**替換**規則集內的已有規則。需要保留原規則時，先匯出或讀取規則，合併後再儲存。

## 協議與端點覆蓋

供應商的“路由規則”控制使用哪個上游協議，與選擇供應商的“模型路由”是不同的配置。

- `operation_rules` 覆蓋某個操作支援的協議列表。用戶端協議在列表中時優先直通，否則嘗試轉換到列表的第一項。
- `operation_endpoints` 覆蓋某個操作、協議和傳輸方式的完整端點 URL。

例如，修改生成操作支援的協議：

```sh
curl -sS -X POST http://127.0.0.1:8787/admin/api/operation-rules \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"providerId":"your-provider-id","operation":"generate_content",
       "action":"dialects","target":["claude","openai_chat"]}'
```

沒有覆蓋時使用渠道預設值。`POST /admin/api/providers/{id}/routing-defaults/reset` 清除該供應商的操作規則和端點覆蓋。

`custom` 渠道還需要在供應商配置中宣告 `dialects`，例如 `["openai_chat", "openai"]`。某個操作無法呼叫時，先核對上游是否支援它，再檢查這個列表和操作覆蓋。
