---
title: "GPROXY 是什麼？"
description: "瞭解 GPROXY 的用途、支援的協議和常用概念。"
---


GPROXY 是一個可自行部署的大模型 API 閘道器。把上游帳戶接入 GPROXY 後，用戶端只需配置閘道器地址、閘道器 API Key 和模型名。

它負責選擇供應商和憑證，在需要時轉換請求與回應格式，執行訪問權限和限額，並記錄用量與費用。GPROXY 不執行模型，實際推理由上游服務完成。

## 適合哪些場景

- 管理多個上游帳戶，希望集中配置憑證和故障切換。
- 給團隊提供固定的模型名，更換供應商時不必修改每個用戶端。
- 讓 Codex CLI、Claude Code 等工具透過統一入口訪問上游，並按使用者記錄用量。
- 在 Rust 應用中嵌入閘道器能力，見[嵌入核心庫](/zh-tw/reference/embedding/)。

## 支援的協議

主要支援 OpenAI Chat Completions、OpenAI Responses、Claude Messages 和 Gemini GenerateContent，包括串流回應。協議不同時，GPROXY 在用戶端和上游格式之間直接轉換。

嵌入、重排、影象、音訊、影片、檔案和 Realtime 等操作也有對應介面，但可用範圍取決於渠道、模型和部署方式。協議轉換不能讓上游獲得它本身不支援的能力。介面清單見[路由與端點](/zh-tw/reference/routing-table/)。

## 常用概念

| 概念 | 含義 |
| --- | --- |
| 渠道（Channel） | 某類上游的介面卡，規定登入方式、請求格式和可用操作，例如 `openai`、`codex`、`custom`。 |
| 供應商（Provider） | 儲存的一條上游連線，包含名稱、渠道、地址、配置和憑證池。 |
| 憑證（Credential） | 用於訪問上游的 API Key、OAuth 令牌或 Cookie。同一個供應商可儲存多份憑證。 |
| 模型路由（Route） | 用戶端使用的模型名，對應一個或多個供應商與上游模型，可配置權重和回退層級。 |
| 閘道器 API Key | GPROXY 簽發給用戶端的金鑰，關聯使用者、權限和費用預算。它不是上游金鑰。 |
| 重寫規則 | 修改系統提示詞、快取斷點、JSON 欄位、文字或請求頭的規則。 |

憑證在配置主金鑰時加密儲存；CLI 未配置主金鑰時使用明文儲存並列印提示。詳見[配置](/zh-tw/reference/configuration/)。

## 選擇部署方式

| 方式 | 適合的用途 |
| --- | --- |
| Application | 在桌面或手機上使用，透過應用內精靈和控制台管理。 |
| CLI / 容器 | 部署到伺服器，提供 HTTP API 和瀏覽器控制台。 |
| Cloudflare Workers | 使用邊緣執行環境和遠端儲存，支援 WebSocket / Realtime。 |

安裝包和平台限制見[安裝](/zh-tw/getting-started/installation/)。從 v3 更新時，請先閱讀[遷移說明](/zh-tw/deployment/v3-to-v4/)。

## 模型名與請求地址

假設建立了 `fast` 路由，用戶端可向 `/v1/chat/completions` 傳送 `model: "fast"`。

也可以透過 `/openai-main/v1/chat/completions` 指定供應商，在請求中填寫上游模型 ID。路由名含名稱空間時，例如 `acme/fast`，可使用 `/acme/v1/chat/completions` 配合 `model: "fast"`。

解析順序和命名限制見[模型與路由](/zh-tw/guides/models/)。第一次使用可以直接跟隨[快速開始](/zh-tw/getting-started/quick-start/)。
