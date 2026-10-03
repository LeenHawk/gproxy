---
title: "用量、日誌與審計"
description: "檢視費用、上下游請求、抓包與配置變更記錄。"
---

控制台把用量、下游請求、上游呼叫和審計分開顯示。它們回答不同的問題：花了多少、用戶端傳送了什麼、實際呼叫了哪個上游，以及誰修改了配置。

## 用量與費用

用量記錄對應一次實際的上游呼叫。一次用戶端請求發生重試或故障切換時，可能產生多條用量記錄。供應商和憑證歸屬、token 數、媒體與工具數量、費用都可獨立於抓包儲存。

單條記錄中，未報告的 token 欄位可能是 `null`，不等於 0。`completeness` 表示用量的完整程度；`metrics` 保留動態維度與協議細節。費用由配置的價格規則計算，不是上游帳戶餘額。

彙總支援按時間、模型、供應商、憑證等條件查詢。`scanned` 與 `truncated` 表示掃描數量和是否觸及上限；截斷的結果不能當作完整賬單。

## 請求日誌

- **下游日誌**記錄用戶端與閘道器之間的請求。
- **上游日誌**記錄閘道器實際發出的呼叫。同一個下游請求可關聯多次上游嘗試。
- **抓包內容**儲存選定的請求、回應或流事件，是否保留正文由設定決定。

HTTP / SSE 與 WebSocket 都有對應的記錄。WebSocket 幀屬於連線上的事件，不應簡單把幀數當成模型呼叫次數。

## 啟用正文記錄

在實例日誌設定中分別控制上游、下游日誌與正文。API 示例：

```sh
curl -sS -X PATCH http://127.0.0.1:8787/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"logging":{"enableUpstreamLogBody":true}}'
```

正文記錄會增加儲存和處理開銷。日誌狀態會區分未捕獲、截斷和完整內容；沒有正文不代表請求沒有傳送。關閉抓包也不等於關閉用量結算。

預設脫敏會遮蔽識別到的金鑰、Cookie 和令牌欄位，但不會識別正文中的所有個人或業務資訊。`disableLogRedaction` 會關閉日誌脫敏，應按實際需要配置。

## 查詢 API

CLI / 容器的 HTTP 管理介面提供：

| 路徑 | 內容 |
| --- | --- |
| `/admin/api/usage` | 用量彙總、分組與趨勢 |
| `/admin/api/usage/records` | 逐次上游呼叫的用量記錄 |
| `/admin/api/logs/downstream` | 下游請求列表 |
| `/admin/api/logs/upstream` | 上游呼叫列表 |
| `/admin/api/logs/downstream/{id}` | 下游請求詳情 |
| `/admin/api/logs/captures/{id}` | 抓包詳情 |
| `/admin/api/audit` | 審計記錄 |

各介面會檢查管理範圍與能力。個人介面 `/portal/api/usage`、`/portal/api/quota` 和 `/portal/api/requests` 限定在呼叫方可見的資料。Application 透過應用內 IPC 查詢，不在 HTTP 連接埠開放這些管理介面。

## 審計

審計記錄管理與 OAuth 操作，包括操作者、動作、結果和時間。`GPROXY_AUDIT_ENABLED=false` 可關閉新增審計，已有記錄仍可查詢；它不關閉請求日誌或用量結算。

## 程序日誌

CLI 使用 `GPROXY_LOG_FILTER` 和 `GPROXY_LOG_FORMAT` 設定日誌級別與格式，程序日誌寫入標準錯誤。首次初始化生成的管理員資訊寫入標準輸出。服務管理器和容器可能同時收集這兩路輸出，請妥善保管首次啟動日誌。

費用規則見[價格與分層](/zh-tw/reference/pricing/)，預算見[權限、限流與費用預算](/zh-tw/guides/permissions/)。
