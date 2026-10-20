---
title: 從 v2 PostgreSQL 遷移到 v4
description: 使用離線遷移工具，將 v2 PostgreSQL 資料匯入新的 v4 PostgreSQL 資料庫。
---

下載同一版本的 [Shell 指令碼](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-postgres.sh)和 [Python 輔助程式](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-postgres.py)，將兩個檔案放在同一目錄。工具針對 v2.3.2 的資料表結構，依照以下路徑離線遷移：

```text
v2 PostgreSQL → 暫存 v2 SQLite → v3.0.22 SQLite → 新的 v4 PostgreSQL
```

來源 PostgreSQL 資料在同一個唯讀、可重複讀取交易中讀取，指令碼不修改原資料庫。快照中的憑證保持加密，由現有 v3/v4 匯入器使用原主金鑰解密並重新加密。兩個步驟都不會啟動 HTTP 服務，指令碼也不會自動下載或安裝 GPROXY 執行檔。

SQLite 檔案只用作匯入中間檔案，不能取代 `pg_dump` 備份：它不會重建 PostgreSQL 的索引、預設值和約束供原資料庫還原使用。

## 下載與準備

先將指令碼儲存到本機：

```sh
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-postgres.sh -o migrate-v2-postgres.sh
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-postgres.py -o migrate-v2-postgres.py
```

需要準備：

- Bash、支援 SQLite 的 Python 3.10+，以及 psycopg 3：

  ```sh
  python3 -m venv .migration-venv
  .migration-venv/bin/pip install 'psycopg[binary]>=3,<4'
  export PYTHON="$PWD/.migration-venv/bin/python"
  ```

- 適合目前平台的 **v3.0.22** 和目前穩定版 v4 執行檔，分別下載、解壓縮到不同目錄：[v3.0.22](https://github.com/LeenHawk/gproxy/releases/tag/v3.0.22)、[穩定版](https://github.com/LeenHawk/gproxy/releases/latest)。v4 必須包含 PostgreSQL 驅動程式。v3 固定使用 3.0.22，因為指令碼適配的是該版本的來源資料結構和命令參數。官方 v4.0.2 Linux CLI 缺少 PostgreSQL 驅動程式，請使用新版穩定 CLI；本工具已用 v4.1.8 驗證。
- 嘗試升級前的完整 `pg_dump` 備份、原主金鑰，以及足夠儲存兩份 SQLite 資料庫的本機磁碟空間，包括請求日誌。v3 匯入器會把用量明細讀入記憶體，需要依歷史記錄數量預留記憶體。
- 一個專用、全新且為空的 PostgreSQL 目標資料庫，使用 `public` schema。匯入前不要對它啟動 GPROXY；匯入器會建立資料表。

最終遷移前，停止所有向來源資料庫寫入的 GPROXY 執行個體。建議先把原始 v2 備份還原到暫存 PostgreSQL 資料庫，再使用只有 SELECT 權限的帳號讀取。如果已經讓 v4 開啟過舊資料庫，請使用升級前的 v2 備份，不要使用混合新舊資料表的資料庫。不需要把資料庫、憑證或主金鑰傳送給他人。

## 1. 準備並檢查中間資料庫

在 shell 中設定來源連線位址和原來的 Base64 格式 `GPROXY_MASTER_KEY`。以下互動輸入方式不會把這些值寫入 shell 歷史記錄：

```sh
read -rsp '來源 PostgreSQL URL：' GPROXY_V2_SOURCE_DSN; echo
export GPROXY_V2_SOURCE_DSN
read -rsp '原主金鑰（明文來源資料庫留空）：' GPROXY_MASTER_KEY; echo
export GPROXY_MASTER_KEY

# v2 資料表不在 public 中時，可指定來源 schema：
# export GPROXY_V2_SCHEMA=your_schema

bash migrate-v2-postgres.sh prepare ./migration-work /absolute/path/to/v3/gproxy
```

`migration-work` 必須尚不存在。指令碼會以私有權限建立它，所有輸出都儲存在裡面。指令碼分批複製來源 schema 中的全部資料表，核對資料列數量，僅在 SQLite 中補齊舊結構缺少的欄位，然後執行 v3 預先檢查、匯入、完整性和用量檢查。舊 `image_price` 保留為按圖片張數計費的 `image_outputs` 價格，同時保留每百萬 token 的價格，不會直接改名成圖片 token 價格。

繼續之前，請檢查：

- `snapshot-report.json`：來源資料表的資料列數量和暫存結構調整。
- `v3-preflight.log`、`v3-import.log`：轉換結果、略過的資料表、權限和路由變化。
- `v3-verification.json`：主要設定數量和歷史費用精確總額。

整個目錄包含資料庫副本和報告，應保持私有。v3.0.22 只能透過 `--from-v2-master-key` 接收來源主金鑰，因此金鑰會短暫出現在 v3 程序參數中。請在可信機器上執行，不要使用 `bash -x`；指令碼本身不會印出金鑰。

## 2. 匯入新的 PostgreSQL 資料庫

繼續使用相同的主金鑰和已經準備好的目錄：

```sh
read -rsp '新的空目標 PostgreSQL URL：' GPROXY_V4_TARGET_DSN; echo
export GPROXY_V4_TARGET_DSN

bash migrate-v2-postgres.sh import ./migration-work /absolute/path/to/v4/gproxy
```

指令碼先核對金鑰指紋並確認目標為空，再呼叫 v4 的 `import --from-v3` 匯入設定、身分和用量。隨後檢查主要實體 ID、密碼雜湊、API key 摘要、用量資料列數量、原請求 ID 和費用，包括 v4 捨入後的實際儲存金額。查看 `v4-import.log` 和 `v4-verification.json`。無法轉換的供應商會使顯式匯入失敗；指令碼沒有啟用略過供應商的選項。

檢查報告後，再將 v4 部署指向新資料庫，繼續使用原主金鑰。恢復流量前確認舊帳號登入、API key、上游憑證、路由、權限和歷史用量正常。不要讓兩套部署同時使用同一份 OAuth refresh token。

## 遷移範圍

工具沿用現有遷移規則，不保證每個 v2 功能的行為完全相同。供應商、憑證、路由、使用者、可恢復的 API key、密碼雜湊和保留的用量明細依這些規則轉換。詳細範圍請參閱 [v2 → v3 說明](/zh-tw/deployment/v2-to-v3/)和 [v3 → v4 說明](/zh-tw/deployment/v3-to-v4/)。

- v4 儲存金額時保留 9 位小數，原始用量費用保存在 `metrics.v3.cost`，驗證使用該精確值。
- 已刪除的用量明細無法從舊彙總資料重建。v2 用量彙總和歷史配額週期保留在快照中，不會匯入。
- 請求日誌會進入暫存 v3 資料庫，但不會變成 v4 封包擷取記錄。工作階段、稽核日誌、憑證健康狀態和快取不會遷入。
- 部分權限、限流、改寫和供應商設定會變化或無法轉換。請閱讀兩階段的報告，並在恢復服務前重新設定必要策略。數量驗證通過不代表策略完全等價。

## 失敗、重試與回復

命令失敗時傳回非零結束碼並保留所有工作檔案。指令碼不會刪除資料庫，也不會自動撤銷部分匯入。`prepare` 不需要目標連線，兩個步驟都不會寫入來源 PostgreSQL 資料庫。

準備失敗時，檢查報告、修復原因，再使用**新的工作目錄**重試。匯入失敗時，保留部分寫入的目標資料庫和日誌以便排查；修復原因後，使用**新的空目標資料庫和新的工作目錄**重新完整執行。指令碼拒絕重用非空目標，不要用 `--merge` 繞過檢查。

切換部署前，可繼續使用原部署和原資料庫。切換後如需回復，停止 v4 並還原原 v2 部署和備份；v4 中新產生的用量不會自動合併回 v2。保留原始備份和工作目錄，直到遷移核對完成且符合備份保留策略。
