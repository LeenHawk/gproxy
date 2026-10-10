---
title: "安裝"
description: "選擇 GPROXY 4.0 的應用、命令列程式、容器或託管平台部署。"
---


日常在電腦上使用，選 **Application**；部署到伺服器，選 **CLI** 或容器。兩者使用相同的閘道器功能，主要區別是如何啟動和管理。

開啟[下載頁](/zh-tw/download/)或 [最新穩定版](https://github.com/LeenHawk/gproxy/releases/latest)，再下載對應系統和架構的檔案。滾動更新的 `nightly` 提供開發快照。

## 選擇下載檔案

| 平台 | CLI（`gproxy-*`） | Application（`gproxy-tauri-*`） |
| --- | --- | --- |
| Linux GNU：x86_64、aarch64、riscv64 | ZIP、DEB | ZIP、DEB |
| Linux musl：x86_64、aarch64、riscv64 | ZIP、DEB | — |
| Windows：x86_64、aarch64 | ZIP、MSIX | ZIP、MSIX |
| macOS：x86_64、aarch64 | ZIP、DMG | ZIP、DMG |
| Android：x86_64、aarch64 | ZIP、Termux DEB | APK |
| OpenHarmony / HarmonyOS NEXT | ARM64、x86_64 ZIP | ARM64 實驗性 HAP |

普通 Intel / AMD 電腦選 x86_64，Apple Silicon 選 aarch64。Linux Application 需要系統提供 WebKitGTK 4.1。

Android CLI 在 Termux 內執行，用 `apt install ./gproxy-android-<architecture>.deb` 安裝 DEB。
使用 ZIP 前先執行 `pkg install libc++ openssl ca-certificates`，解壓到 Termux 主目錄，
並按 `TERMUX.txt` 操作。程式自更新已禁用，透過 APT 安裝新版 DEB；啟用的軟體倉庫收錄後
才可使用 `pkg upgrade gproxy`。升級時保留原資料目錄和主金鑰。

Windows（x64、ARM64）可直接從 Microsoft Store 安裝兩個版本：

| GPROXY Desktop | GPROXY CLI |
| :---: | :---: |
| <a href="https://apps.microsoft.com/detail/9P2FJRB9RS4Z?mode=direct"><img src="https://get.microsoft.com/images/zh-tw%20dark.svg" alt="從 Microsoft 獲取：GPROXY Desktop" height="48"></a> | <a href="https://apps.microsoft.com/detail/9NBMH3S5K0L9?mode=direct"><img src="https://get.microsoft.com/images/zh-tw%20dark.svg" alt="從 Microsoft 獲取：GPROXY CLI" height="48"></a> |

GPROXY Desktop 即 Application 版本。商店版本由微軟簽名，並透過商店自動更新。也可以用 `winget install --id 9P2FJRB9RS4Z --source msstore`（Desktop）或 `winget install --id 9NBMH3S5K0L9 --source msstore`（CLI）安裝。

Windows Release 附件中的 MSIX 是商店提交包，可能尚未帶有微軟簽名；請從商店安裝，或選擇 ZIP。macOS 應用使用 ad-hoc 簽名，尚未公證。鴻蒙 HAP 需要自行簽名，尚未完成真機驗證，也不提供背景服務。

CI 環境可選無前端 CLI：不打包 Web 控制台，保留代理和管理 API。Release 工作流程發布 Linux、Windows、macOS 和 Android（Termux）的 x86_64 / aarch64 ZIP 包，Linux 還提供 riscv64，三種架構均有 GNU 和 musl 版本；下載位置和構建命令見[無前端構建](/zh-tw/deployment/release-build/#ci-無前端版)。

## Application：在介面中設定

安裝或解壓 Application 後，啟動 GPROXY。全新實例會開啟設定精靈：

1. **實例設定**：選擇監聽地址、連接埠、資料目錄和資料庫。初次使用可保留預設值。
2. **管理員帳戶**：設定使用者名稱和至少 8 個字元的密碼。API Key 可自行填寫，也可留空生成。
3. **匯入配置**：可選匯入已有的 v4 配置檔案，沒有檔案時直接完成。

完成後儲存頁面顯示的服務地址和 API Key，再進入控制台。桌面版可選擇開機自啟和系統托盤；移動版使用應用私有資料目錄，具體選項以平台為準。Android 精靈還提供通知與背景執行權限入口。

應用視窗透過本機 IPC 管理實例，HTTP 監聽連接埠供用戶端呼叫閘道器 API。瀏覽器訪問這個連接埠不會開啟應用內精靈或管理控制台。

## CLI：啟動伺服器

解壓 CLI ZIP，在終端進入所在目錄。Linux / macOS 執行：

```sh
chmod +x ./gproxy
./gproxy serve --data-dir ./data --port 8787
```

Windows 在 PowerShell 中執行：

```powershell
.\gproxy.exe serve --data-dir .\data --port 8787
```

全新實例會在終端顯示生成的管理員密碼和 API Key，**只顯示一次，請儲存**。已有實例重啟時，顯式設定的 `GPROXY_ADMIN_PASSWORD` 優先更新同名使用者的密碼；沒有同名使用者時恢復並重新命名 0 號管理員；未設定則保留現狀。

開啟 **http://127.0.0.1:8787/console/**，使用管理員帳戶登入。個人頁面和管理頁面都在這個控制台內。CLI 不使用 Application 的設定精靈。

預設只監聽本機，SQLite 資料庫位於 `./data/gproxy.db`。如果要接受其他裝置的連線，需要調整監聽地址，並配置網路訪問與 HTTPS。主金鑰、環境變數和資料庫選項見[配置](/zh-tw/reference/configuration/)。

CLI 需要透過 `GPROXY_MASTER_KEY` 配置憑證加密主金鑰；未配置時，上游憑證按明文儲存，啟動日誌會提示。請在新增憑證前設定並妥善儲存主金鑰，後續啟動使用同一值。

## 容器

釋出流水線將映像同步到 `ghcr.io/leenhawk/gproxy` 和 Docker Hub 的 `leenhawk/gproxy`。穩定版本使用 `vX.Y.Z` 標籤，beta 使用 `staging`，開發快照使用 `nightly`；musl 映像在標籤後加 `-musl`。請選擇已釋出的 v4 標籤，不要用舊的 v3 標籤啟動 v4。

```sh
export GPROXY_IMAGE='leenhawk/gproxy:<tag>'
# GPROXY_MASTER_KEY 应是已保存的 32 字节主密钥（64 位十六进制或 base64）。
docker run -d --name gproxy --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v gproxy-data:/app/data \
  -e GPROXY_MASTER_KEY \
  "$GPROXY_IMAGE"
docker logs gproxy
```

從首次啟動日誌儲存管理員資訊，再開啟 `/console/`。映像以 `65532:65532` 執行；使用宿主機目錄掛載時，需要給該使用者寫權限。資料儲存在 `/app/data`，更新容器時保留資料卷。釋出流水線構建 amd64、arm64、riscv64 的 GNU 和 musl 映像。

## 託管平台

Cloudflare Workers、Netlify 和 Vercel 都提供部署模板，無需本機 Rust 工具鏈。Cloudflare 可用 D1 或 libSQL/Turso，其他兩個平台使用 PostgreSQL；需要 WebSocket / Realtime 時可選 Cloudflare 或 Vercel（WebSocket Beta）。

部署按鈕、資料庫選擇和首次登入步驟統一見[託管平台部署](/zh-tw/deployment/edge/)。需要手動部署 Workers 時，同一頁也提供釋出包和 Wrangler 的操作步驟。

## 從 v3 升級

先備份資料庫、主金鑰和啟動配置，停止舊程序，再用原配置啟動 v4。支援的 v3 SQLite、PostgreSQL、MySQL 和 D1 資料庫會自動遷移；請閱讀[遷移說明](/zh-tw/deployment/v3-to-v4/)，確認保留的資料和需要檢查的遷移報告。

需要自行編譯時，見[從原始碼構建](/zh-tw/deployment/release-build/)。安裝完成後繼續[快速開始](/zh-tw/getting-started/quick-start/)。
