---
title: "快速開始"
description: "在控制台新增供應商和憑證，配置模型路由併傳送第一個請求。"
---


先按[安裝說明](/zh-tw/getting-started/installation/)啟動 GPROXY。Application 使用者完成首次設定後進入應用內控制台；CLI 和容器使用者在瀏覽器開啟 `http://127.0.0.1:8787/console/`，使用首次啟動的管理員帳戶登入。

下面以 OpenAI 相容服務為例。你需要一份可用的上游 API Key，以及該服務支援的模型名稱。

## 1. 新增供應商

進入 **供應商**，新增一條連線：

- 名稱填寫 `openai-main`。
- 使用 OpenAI 官方服務時選擇 `openai` 渠道。
- 使用其他 OpenAI 相容服務時選擇 `custom`，填寫服務地址，並按上游文件選擇支援的協議，例如 Chat Completions（`openai_chat`）。

供應商名稱會用於直接呼叫的地址。不同供應商可以使用同一種渠道，分別儲存各自的地址和憑證。

## 2. 新增憑證並測試

開啟剛建立的供應商，在 **憑證** 標籤中新增上游 API Key。使用 Codex、Claude Code 等登入渠道時，選擇登入新增憑證，按頁面提示完成授權。

在憑證行開啟測試，選擇上游支援的模型併傳送一條簡短訊息。生成測試會真實呼叫上游，可能產生費用。成功後記下模型 ID，下一步會用到。

上游 API Key 儲存在 GPROXY；用戶端使用的是 GPROXY 簽發的閘道器 API Key，兩者不要混用。

## 3. 建立模型路由

進入 **模型路由**，新建名稱為 `fast` 的路由，新增一個成員：供應商選擇 `openai-main`，上游模型填寫剛才測試成功的模型 ID，層級和權重先保留預設值。

用戶端以後填寫 `fast` 即可。更換上游時只需修改路由成員。一個供應商也可以直接呼叫，不必建立路由，見[模型與路由](/zh-tw/guides/models/)。

## 4. 傳送請求

使用首次設定時儲存的閘道器 API Key，或在 **我的帳戶 → 金鑰** 建立一把新金鑰。替換下面的佔位值；如果修改過連接埠，也要修改地址。

```sh
export GPROXY_KEY='your-gproxy-api-key'
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"你好，请简单介绍自己。"}]}'
```

如果用戶端要求填寫 OpenAI Base URL，通常填 `http://127.0.0.1:8787/v1`；如果要求填寫完整請求 URL，則填示例中的 `/v1/chat/completions` 地址。

其他協議和串流呼叫見[傳送第一個請求](/zh-tw/getting-started/first-request/)。Codex CLI、Claude Code 等工具的專用配置見[CLI 用戶端](/zh-tw/guides/cli-clients/)。

## 5. 檢視用量

在控制台檢視請求和用量記錄，確認模型、供應商、token 數與費用。費用依賴配置的價格規則，不等於上游賬單的實時餘額。上游未報告的用量欄位可能為空。

## 遇到問題

| 現象 | 先檢查 |
| --- | --- |
| 無法連線 | GPROXY 是否執行，地址和連接埠是否正確 |
| `401` | 是否使用閘道器 API Key，金鑰是否有效 |
| `403` | 當前使用者或金鑰是否有訪問目標模型的權限 |
| `404 unknown_model` | `fast` 路由是否存在並啟用，是否新增了成員 |
| `429` | 本地限額與上游額度，憑證是否暫時不可用 |
| 上游錯誤或不支援該操作 | 憑證測試結果、模型 ID、供應商協議與端點配置 |

需要用指令碼管理時，[供應商](/zh-tw/guides/providers/)和[模型路由](/zh-tw/guides/models/)文件中提供了管理 API 示例。
