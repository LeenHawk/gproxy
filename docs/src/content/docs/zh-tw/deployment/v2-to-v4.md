---
title: 從 v2 遷移到 v4
description: 使用一個指令碼將 v2 資料遷移到 v4，支援 SQLite、PostgreSQL 與 MySQL。
---

使用統一的 [migrate-v2-to-v4.py](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-to-v4.py)。來源與目標都支援 SQLite、PostgreSQL、MySQL，可跨資料庫遷移。

指令碼一次完成：**v2 來源 → SQLite 快照 → v3 SQLite → v4 SQLite → 目標資料庫**。來源唯讀，帳號、憑證、API key 和用量先由既有升級器轉換，再將完整的 v4 資料搬到目標。

## 準備

停止舊執行個體並保留資料庫備份。目標必須是**新的 SQLite 檔案或空的 PostgreSQL/MySQL 資料庫**，不可填入原資料庫位址。加密執行個體繼續使用原來的 `GPROXY_MASTER_KEY`。

準備 Python 3.10+、[v3.0.22](https://github.com/LeenHawk/gproxy/releases/tag/v3.0.22) 和 [v4](https://github.com/LeenHawk/gproxy/releases/latest) 執行檔。v4 必須包含目標資料庫驅動程式。純 SQLite 不需額外 Python 套件；使用 PostgreSQL 或 MySQL 時安裝對應驅動程式：

```sh
python3 -m pip install 'psycopg[binary]>=3,<4'  # PostgreSQL
python3 -m pip install 'PyMySQL[rsa]>=1,<2'     # MySQL
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-to-v4.py -o migrate-v2-to-v4.py
```

## 執行一次遷移

SQLite 範例：

```sh
python3 migrate-v2-to-v4.py \
  --source ./old-data/gproxy.db \
  --target ./new-data/gproxy.db \
  --v3-bin ./v3/gproxy \
  --v4-bin ./v4/gproxy
```

將 `--source`、`--target` 換成 `postgresql://user:password@host/database` 或 `mysql://user:password@host/database` 即可使用遠端資料庫，兩端可不同。含密碼的連線字串可透過 `GPROXY_SOURCE_DSN`、`GPROXY_TARGET_DSN` 環境變數提供，並省略對應參數。原主金鑰也透過環境變數設定：

```sh
read -rsp '來源資料庫連線字串：' GPROXY_SOURCE_DSN; echo; export GPROXY_SOURCE_DSN
read -rsp '空目標資料庫連線字串：' GPROXY_TARGET_DSN; echo; export GPROXY_TARGET_DSN
read -rsp '原主金鑰（明文來源留空）：' GPROXY_MASTER_KEY; echo; export GPROXY_MASTER_KEY
python3 migrate-v2-to-v4.py --v3-bin ./v3/gproxy --v4-bin ./v4/gproxy
```

指令碼會建立工作目錄並顯示位置；可用 `--work-dir` 指定尚不存在的目錄。預設從 PATH 尋找 `gproxy-v3` 和 `gproxy`，找到時可省略執行檔路徑。PostgreSQL 來源不在 `public` 時使用 `--source-schema`；目標 PostgreSQL 使用 `public`。MySQL 來源資料表必須使用 InnoDB；DSN 可用 `ssl-mode`、`ssl-ca`、`ssl-cert`、`ssl-key` 設定 TLS。

## 完成後

中間資料庫與每階段報告均保留在工作目錄。指令碼檢查帳號、API key 摘要和歷史費用，並對最終 PostgreSQL/MySQL 搬運逐表比對筆數及資料列內容校驗和。檢查 `v3-import.log`、`v4-import.log` 和 `*-verification.json` 後，讓 v4 使用目標資料庫與原主金鑰，再恢復流量。

轉換範圍沿用既有升級器：部分舊權限與規則需要重設；保留的用量明細會遷移，但已刪除的明細無法從彙總重建。舊請求記錄、工作階段、稽核與快取不會變成 v4 對應記錄。v4 金額保留 9 位小數，精確歷史費用仍保存在 `metrics.v3.cost`。請參閱 [v2 → v3](/zh-tw/deployment/v2-to-v3/)和 [v3 → v4](/zh-tw/deployment/v3-to-v4/)。

失敗時停止並保留現場，不刪除來源或自動清空目標。排查後使用新的工作目錄與空目標重試。工作目錄含資料庫與秘密，應保持私有；v3.0.22 的金鑰會短暫出現在程序參數中，不要使用 shell 追蹤。
