---
title: "傳送第一個請求"
description: "使用 OpenAI、Claude 和 Gemini API 呼叫閘道器，啟用串流回應並排查常見錯誤。"
---

完成[快速開始](/zh-tw/getting-started/quick-start/)後，應該已有名為 `fast` 的模型路由和一把閘道器 API Key。下面的例子使用預設地址；修改過連接埠時請一起調整。

```sh
export GPROXY_KEY='your-gproxy-api-key'
```

閘道器接受 `Authorization: Bearer <key>`、`x-api-key` 和 `x-goog-api-key`。請使用閘道器簽發的金鑰，不要填上游憑證。

## OpenAI Chat Completions

```sh
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

## OpenAI Responses

```sh
curl -sS http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","input":"Say hello."}'
```

## Claude Messages

```sh
curl -sS http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: $GPROXY_KEY" \
  -H 'anthropic-version: 2023-06-01' \
  -H 'content-type: application/json' \
  -d '{"model":"fast","max_tokens":256,
       "messages":[{"role":"user","content":"Say hello."}]}'
```

## Gemini GenerateContent

```sh
curl -sS "http://127.0.0.1:8787/v1beta/models/fast:generateContent" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Say hello."}]}]}'
```

## 串流回應

```sh
curl -sSN http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","stream":true,
       "messages":[{"role":"user","content":"Count to three."}]}'
```

## Gemini 串流回應

```sh
curl -sSN "http://127.0.0.1:8787/v1beta/models/fast:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Count to three."}]}]}'
```

返回格式與用戶端請求的協議一致。能否呼叫成功取決於路由成員、上游能力與轉換支援；某個上游不能執行的操作，不會因為更換介面路徑而自動獲得支援。

## 模型列表

控制台中的模型目錄是配置閘道器模型名的入口。透過 API 查詢呼叫方可見的閘道器目錄：

```sh
curl -sS http://127.0.0.1:8787/portal/api/models \
  -H "Authorization: Bearer $GPROXY_KEY"
```

目錄中的 `permitted` 表示當前呼叫方是否有權限。`GET /v1/models` 轉發的是所選上游的模型列表，與閘道器目錄不同。Application 不在 HTTP 連接埠開放 `/portal/api`，請在應用內檢視目錄。

## 指定供應商或名稱空間

| 地址 | 請求中的模型名 | 目標 |
| --- | --- | --- |
| `/v1/chat/completions` | `fast` | 名為 `fast` 的路由 |
| `/openai-main/v1/chat/completions` | 上游模型 ID | `openai-main` 供應商 |
| `/acme/v1/chat/completions` | `fast` | 名為 `acme/fast` 的路由 |

命名規則見[模型與路由](/zh-tw/guides/models/)。

## 常見錯誤

| 狀態碼 | 含義 |
| --- | --- |
| `400` | 引數格式或配置值無效 |
| `401` | 金鑰缺失、無效、停用或過期 |
| `403` | 權限或 OAuth 授權範圍不足 |
| `404` | 模型、路由或資源不存在 |
| `429` | 請求受到限流或限額約束 |
| `5xx` | 閘道器或上游呼叫失敗，需結合日誌檢查 |

請求完成後可在控制台檢視用量與請求記錄。正文是否儲存取決於日誌設定，詳見[用量、日誌與審計](/zh-tw/guides/observability/)。
