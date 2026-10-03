---
title: "使用者與 API Key"
description: "建立使用者、分配管理範圍，簽發和輪換閘道器 API Key。"
---


控制台使用使用者名稱和密碼登入，用戶端使用閘道器 API Key 呼叫模型。上游憑證在供應商中配置，不能用來登入 GPROXY。

## 首個管理員

Application 在首次設定精靈中建立管理員。CLI 在空實例啟動時建立管理員並顯示生成的密碼和 API Key，也可以透過以下變數預先指定：

```sh
export GPROXY_ADMIN_USER='admin'
export GPROXY_ADMIN_PASSWORD='your-initial-password'
export GPROXY_BOOTSTRAP_ADMIN_API_KEY='your-initial-gateway-key'
./gproxy serve
```

空庫初始化時，密碼和金鑰變數均可省略，由程式生成。**顯式設定的 `GPROXY_ADMIN_PASSWORD` 會在啟動時優先更新同名使用者的密碼；沒有同名使用者時恢復並重新命名 0 號管理員；API Key 選項只用於首次初始化。** 不再支援透過 `GPROXY_BOOTSTRAP_CHANNELS` 自動建立供應商，請在控制台新增。

CLI / 容器的登入入口是 `/console/`。個人頁和管理頁共用登入會話，HTTP 會話 Cookie 名為 `gproxy_session`，生命週期由 `session_ttl_secs` 配置。Application 使用應用內控制台。

## 使用者、組織與團隊

實例管理員在訪問控制中建立使用者。使用者角色為 `admin` 或 `user`；未設定密碼的使用者不能登入控制台，但可以使用獲准簽發的 API Key。

組織與團隊透過成員關係分配使用者和管理角色。API Key 可以繫結組織或團隊，這個繫結用於確定憑證可見範圍和費用預算。不能透過在請求中偽造組織、團隊請求頭來改變繫結。

組織和團隊管理員只能管理其範圍內開放的功能，不等於實例管理員。具體入口見[控制台與範圍管理](/zh-tw/guides/console/)。

## 建立金鑰

個人金鑰在 **我的帳戶 → 金鑰** 管理；實例管理員也可以在 **訪問控制 → 使用者金鑰** 操作。

| 設定 | 含義 |
| --- | --- |
| 名稱 | 方便區分不同應用或裝置 |
| 歸屬 | 關聯使用者，以及可選的組織或團隊 |
| 有效期與啟用狀態 | 過期或停用後拒絕新請求 |
| 管理權限 | 允許金鑰訪問管理操作，預設關閉；仍受所屬使用者和範圍權限約束 |
| 保留金鑰 | 允許以後再次檢視完整值，預設不保留 |
| 費用預算 | 管理員可配置金額、週期和模型範圍 |

建立後立即複製完整金鑰。沒有保留金鑰原文、或僅從舊實例匯入摘要的金鑰，不能再次顯示。顯示操作仍需相應權限。

預算與新金鑰可以一起儲存，預算校驗失敗不會單獨建立金鑰。

## 輪換與撤銷

金鑰支援輪換，輪換後舊值失效，需要更新用戶端。如果希望逐個遷移用戶端而不中斷舊配置，可以先建立第二把金鑰，替換完成後再停用或刪除舊金鑰。

管理 API 的對應操作為：

- `POST /admin/api/api-keys/{id}/rotate`：輪換。
- `GET /admin/api/api-keys/{id}/secret`：檢視已保留的金鑰。
- 個人 API 使用 `/portal/api/keys/{id}/rotate` 和 `/portal/api/keys/{id}/secret`。

## 傳送金鑰

```text
Authorization: Bearer <gateway-key>
x-api-key: <gateway-key>
x-goog-api-key: <gateway-key>
```

選擇用戶端協議慣用的請求頭即可，不要同時填寫不同的金鑰。管理 API 需要金鑰啟用管理權限；普通推理金鑰不會因為所屬使用者是管理員就自動獲得管理權限。

## OAuth 會話

GPROXY 可作為 OAuth 簽發端，讓用戶端經使用者授權取得訪問令牌。OAuth 內部金鑰與普通使用者金鑰不同，不能把它當作普通 Bearer Key 建立或顯示。授權會話可以在控制台撤銷。

用戶端配置和授權流程見[CLI 用戶端](/zh-tw/guides/cli-clients/)。
