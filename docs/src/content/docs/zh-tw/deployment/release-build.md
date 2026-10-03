---
title: "從原始碼構建"
description: 從原始碼構建 GPROXY v4 的每個目標——server、桌面宿主、Worker 與 console——並執行 CI 跑的那幾道質量閘門。
---

`dev` 分支開發新功能，推送後只更新 `nightly`（dev 更新通道）。`main` 分支維護已釋出功能和修復 bug，推送後只更新 `staging`（beta 更新通道）。
正式版本從 `main` 釋出，同時更新 release 和 beta，不覆蓋獨立開發的 dev 通道。版本 tag 必須與 workspace 版本一致。
推送 dev 前執行 `bash scripts/push-dev.sh`，先 rebase 到最新 main；使用 `git config core.hooksPath .githooks` 啟用本地推送檢查。
自動打包由 `.github/workflows/release.yml` 驅動。

## 釋出包

CLI（`gproxy-*`）和 Application（`gproxy-tauri-*`）是兩類獨立程式，分別提供
便攜包和安裝包。Application 內嵌構建好的 Console。

| 平台 | CLI | Application |
| --- | --- | --- |
| Linux GNU（x86_64、aarch64、riscv64） | ZIP、DEB | ZIP、DEB |
| Linux musl（x86_64、aarch64、riscv64） | ZIP、DEB | — |
| Windows（x86_64、aarch64） | ZIP、MSIX | ZIP、MSIX |
| macOS（x86_64、aarch64） | ZIP、DMG | ZIP、DMG |
| Android（x86_64、aarch64） | ZIP、Termux DEB | 僅 APK |
| OpenHarmony / HarmonyOS NEXT | ARM64 / x86_64 ZIP | ARM64 實驗性未簽名 HAP |

Linux CLI DEB 將 `gproxy` 安裝到 `/usr/bin`；Termux DEB 安裝到
`/data/data/com.termux/files/usr`。Android CLI 使用 `distribution/termux/` 中的原始碼配方
和固定版本的 Termux 官方構建器。DEB 和 ZIP 都依賴 Termux 的 `libc++`、OpenSSL 和
CA 證書；ZIP 包含啟動指令碼、二進位制及安裝更新說明 `TERMUX.txt`。
這些構建禁用程式自更新，透過 APT 安裝新版 DEB；啟用的軟體倉庫收錄後可使用
`pkg upgrade gproxy`。Windows CLI MSIX 使用獨立的
`.CLI` 包身份和控制台命令別名；macOS CLI DMG 包含二進位制及終端安裝說明。
Application ZIP 保留桌面資源，macOS ZIP 包含完整 `.app`。

鴻蒙構建使用快取工具鏈映像和固定版本的實驗性 Tauri 分支，其他平台繼續使用穩定版。
HAP 安裝前需要簽名，尚未做真機驗證；暫不提供背景服務。

nightly 附件使用固定名稱，提交 SHA 記錄在更新清單中。Linux x86_64 在 Ubuntu 22.04 構建，
ARM64 在 Ubuntu 24.04 構建，安裝時需要發行版提供 WebKitGTK 4.1。
macOS 使用 ad-hoc 簽名，尚未接入 Developer ID 簽名和公證。

Windows 使用 Microsoft Store 的包身份。`release` 環境需配置四個變數：
`MS_STORE_IDENTITY_NAME`、`MS_STORE_DISPLAY_NAME`、`MS_STORE_IDENTITY_PUBLISHER`、
`MS_STORE_PUBLISHER_DISPLAY_NAME`。正式版保留兩架構的 Store 提交包；啟用
`MS_STORE_PUBLISH_ENABLED` 後，GitHub Release 釋出成功會觸發 Store 提交流程。
GitHub 附帶的 MSIX 與提交包一樣未簽名，由 Store 簽發後分發；它不是可直接雙擊安裝的
可信簽名包。應用依賴系統的 WebView2 Runtime。

Android 使用已有 `ANDROID_SIGNING_*` secrets 簽名並驗證 APK。應用的
`<target-triple>-tauri-apk` 條目進入 Ed25519 簽名更新清單，與 CLI ZIP 更新分開；不再構建舊服務包裝 APK。缺少必需的金鑰或 Store 身份會使打包失敗。

本地先構建 Console 並同步資源，然後呼叫釋出指令碼：

```sh
pnpm --dir console build
node console/scripts/sync-to-embed.mjs
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
TARGET_OS=linux TARGET_TRIPLE=x86_64-unknown-linux-gnu \
  ARTIFACT_NAME=gproxy-tauri-linux-x86_64 \
  GPROXY_BUILD_VERSION=$(scripts/release-metadata.sh version) \
  scripts/package-tauri-release.sh
```

產物寫入 `dist/release/`。工作流還發布原生伺服器 ZIP、Termux 包、Edge 包和
GNU/musl 容器映像；每個應用包都生成構建證明，GitHub 上可查詢 attestations。

## 構建環境

需要 stable Rust（edition 2024）、Go、Clang、Node.js 22.12+（推薦 24 LTS）和 pnpm。Linux 桌面構建還需要 `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0` 開發包。其他平台工具鏈由釋出工作流配置。

## 構建 CLI 與控制台

從倉庫根目錄執行：

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build
cargo build -p gproxy --release
./target/release/gproxy serve --data-dir ./data
```

控制台構建會自動同步資源到 HTTP 宿主和 Tauri 的嵌入目錄，隨後編譯的二進位制包含控制台。只執行 Rust 構建的全新檢出沒有這些資源，`/console` 會返回 404。也可以使用 `--console-path console/dist` 指向單獨構建的資源。

CLI 預設啟用全部渠道、記憶體快取、本地檔案儲存和內建詞表。可按需縮減渠道或增加後端：

```sh
cargo build -p gproxy --release --no-default-features \
  --features embedded-console,memory,fs,codex,claudecode,openai,custom
```

PostgreSQL、MySQL、Redis 和 S3 分別需要 `postgres`、`mysql`、`redis`、`s3` feature。SQLite 始終可用。

### CI 無前端版

Release 工作流程獨立構建 `gproxy-headless-linux-x86_64.zip` 和
`gproxy-headless-linux-aarch64.zip`，無需等待前端構建。在
[Releases](https://github.com/LeenHawk/gproxy/releases) 下載對應包：
開發版選 `nightly`，beta 選 `staging`，穩定版選正式版本。包作為 Release 附件發布，
自更新也會選擇對應的無前端包。

此版本不打包 Web 控制台，保留代理、管理 API、全部渠道、SQLite、記憶體快取、
本地檔案儲存和內建詞表，透過 CLI 或管理 API 配置。原始碼構建不需要 Node.js、
pnpm 或桌面函式庫：

```sh
cargo build --locked --release -p gproxy --bin gproxy \
  --no-default-features --features channels,memory,fs,bundled-vocabulary
./target/release/gproxy serve --console=false
```

不啟用 `embedded-console` 時，即使工作目錄已有前端產物，也不會嵌入二進位檔。
`--console=false` 同時關閉外部控制台目錄的服務。自訂 feature 構建若需要
內嵌控制台，新增 `embedded-console`。

## 構建 Application

構建控制台後執行：

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

全新實例會開啟設定精靈。視窗透過 IPC 管理實例，HTTP 監聽供外部用戶端呼叫閘道器。移動端打包請使用釋出工作流中的平台工具鏈與打包指令碼。

預設 workspace 成員不包含 Tauri，便於沒有桌面依賴的環境編譯後端；`--workspace` 會包含它。

## 構建 Workers

從倉庫根目錄執行：

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

`worker-build` 完成 WASM 編譯與最佳化；`check` 使用 Wrangler 做部署預檢，不釋出到線上。配置和執行限制見[邊緣部署](/zh-tw/deployment/edge/)。

## 檢查改動

後端預設成員：

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

桌面宿主單獨檢查：

```sh
cargo clippy -p gproxy-host-tauri --all-targets -- -D warnings
cargo test -p gproxy-host-tauri
```

WASM 檢查只包含支援該目標的庫和宿主，不能對包含 CLI、Tauri 的整個 workspace 執行：

```sh
cargo check --target wasm32-unknown-unknown \
  -p gproxy-protocol -p gproxy-store -p gproxy-seaorm -p gproxy-client \
  -p gproxy-cache -p gproxy-channel -p gproxy-core -p gproxy-file \
  -p gproxy-tokenizer -p gproxy-sdk -p gproxy-app -p gproxy-host-axum \
  -p gproxy-host-edge
```

控制台與文件：

```sh
pnpm --dir console lint
pnpm --dir console test
pnpm --dir console build
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs check
pnpm --dir docs build
bash scripts/check-docs.sh
```

修改 Rust DTO 後，用 `pnpm --dir console types` 更新生成的 TypeScript 型別。

## 釋出

版本 tag 必須與 workspace 版本一致，並提供 `docs/release-notes/v<版本>.md`。釋出流程構建包、生成簽名更新清單並上傳附件；構建或簽名要求未滿足時，不能視為釋出完成。

版本 tag 還會呼叫 `scripts/publish-crates.sh` 釋出選定的 MIT 庫，其他 crate 使用 git 或路徑依賴。詳情見[嵌入核心庫](/zh-tw/reference/embedding/)。
