#!/bin/sh
# GPROXY CLI installer for Linux, macOS, Android (Termux) and OpenHarmony.
#
#   curl -fsSL https://gproxy.leenhawk.com/install.sh | sh
#   curl -fsSL https://gproxy.leenhawk.com/install.sh | sh -s -- --edition headless -y
#
# Windows: irm https://gproxy.leenhawk.com/install.ps1 | iex
# The desktop and mobile Application is not installed by this script; choose
# it in the menu to be pointed at the download page.
set -eu

SITE='https://gproxy.leenhawk.com'

lang=''
edition=''
target=''
channel='release'
mirror=''
bindir=''
assume_yes=0
configure=''
home_dir=''
db_path=''
dsn=''
listen_host=''
listen_port=''
admin_user=''
admin_password="${GPROXY_ADMIN_PASSWORD:-}"
proxy=''
service=''

usage() {
  cat <<'EOF'
Usage: install.sh [options]

Package
  --edition cli|headless|app  cli: with the Web console (default)
                              headless: no console, for servers and CI
                              app: show where to download the Application
  --target TARGET             override platform detection, e.g. linux-x86_64,
                              linux-aarch64-musl, macos-aarch64,
                              android-aarch64, ohos-x86_64
  --channel release|beta|dev  update channel (default: release)
  --mirror github|gitlab|cnb  download source (default: cnb for zh-CN, else github)
  --dir DIR                   directory for the gproxy command

Instance (written to <data-dir>/.env, mode 600)
  --data-dir DIR              instance directory (default: ~/.local/share/gproxy)
  --db-path FILE              SQLite database file (default: <data-dir>/gproxy.db)
  --dsn URL                   postgres:// or mysql:// connection string instead of SQLite
  --host ADDRESS              listen address (default: 127.0.0.1)
  --port PORT                 listen port (default: 8787)
  --admin-user NAME           administrator name (default: admin)
  --admin-password PASSWORD   administrator password, at least 8 characters
                              (or GPROXY_ADMIN_PASSWORD; generated when empty)
  --proxy URL                 global upstream proxy, e.g. http://127.0.0.1:7890
                              or socks5h://127.0.0.1:1080
  --service / --no-service    register a background service (default: no)
  --no-configure              install the binary only

  --lang en|zh-CN|zh-TW       installer language (default: from $LANG)
  -y, --yes                   no prompts; accept detected values and defaults
  -h, --help                  show this help
EOF
}

while [ $# -gt 0 ]; do
  option="$1"
  case "$option" in
    --*=*) value="${option#*=}"; option="${option%%=*}"; shift ;;
    --edition|--target|--channel|--mirror|--dir|--data-dir|--db-path|--dsn|--host|--port|--admin-user|--admin-password|--proxy|--lang)
      [ $# -ge 2 ] || { echo "missing value for $option" >&2; exit 2; }
      value="$2"; shift 2 ;;
    *) value=''; shift ;;
  esac
  case "$option" in
    --edition) edition="$value" ;;
    --target) target="$value" ;;
    --channel) channel="$value" ;;
    --mirror) mirror="$value" ;;
    --dir) bindir="$value" ;;
    --data-dir) home_dir="$value" ;;
    --db-path) db_path="$value" ;;
    --dsn) dsn="$value" ;;
    --host) listen_host="$value" ;;
    --port) listen_port="$value" ;;
    --admin-user) admin_user="$value" ;;
    --admin-password) admin_password="$value" ;;
    --proxy) proxy="$value" ;;
    --lang) lang="$value" ;;
    --service) service=1 ;;
    --no-service) service=0 ;;
    --no-configure) configure=0 ;;
    -y|--yes) assume_yes=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $option" >&2; usage >&2; exit 2 ;;
  esac
done

# Prompts read the terminal, not stdin, so `curl ... | sh` stays interactive.
interactive=0
if [ "$assume_yes" -eq 0 ] && [ -r /dev/tty ] && (exec </dev/tty) 2>/dev/null; then
  interactive=1
fi

ask() { # ask VAR PROMPT DEFAULT
  answer=''
  if [ -n "$3" ]; then printf '%s [%s]: ' "$2" "$3" >/dev/tty; else printf '%s: ' "$2" >/dev/tty; fi
  read -r answer </dev/tty || answer=''
  [ -n "$answer" ] || answer="$3"
  eval "$1=\$answer"
}

ask_secret() { # ask_secret VAR PROMPT
  answer=''
  printf '%s: ' "$2" >/dev/tty
  stty -echo </dev/tty 2>/dev/null || true
  read -r answer </dev/tty || answer=''
  stty echo </dev/tty 2>/dev/null || true
  printf '\n' >/dev/tty
  eval "$1=\$answer"
}

# ---------------------------------------------------------------- messages

detect_lang() {
  for value in "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}" "${LANGUAGE:-}"; do
    [ -n "$value" ] || continue
    case "$value" in
      zh_TW*|zh_HK*|zh_MO*|zh-TW*|zh-HK*|zh-MO*|zh_Hant*|zh-Hant*) echo zh-TW; return ;;
      zh*) echo zh-CN; return ;;
      C|C.*|POSIX) continue ;;
      *) echo en; return ;;
    esac
  done
  echo en
}

normalize_lang() {
  case "$1" in
    zh-CN|zh-cn|zh_CN|zh|cn|2) echo zh-CN ;;
    zh-TW|zh-tw|zh_TW|tw|3) echo zh-TW ;;
    en|EN|1) echo en ;;
    *) echo '' ;;
  esac
}

# msg KEY -> localized printf format.
msg() {
  case "$lang:$1" in
    en:detected) echo 'Detected platform: %s' ;;
    zh-CN:detected) echo '检测到的平台：%s' ;;
    zh-TW:detected) echo '偵測到的平台：%s' ;;
    en:keep_target) echo 'Use this platform? (Y/n)' ;;
    zh-CN:keep_target) echo '使用此平台？(Y/n)' ;;
    zh-TW:keep_target) echo '使用此平台？(Y/n)' ;;
    en:pick_target) echo 'Choose a platform' ;;
    zh-CN:pick_target) echo '选择平台' ;;
    zh-TW:pick_target) echo '選擇平台' ;;
    en:unknown_platform) echo 'Could not detect a supported platform (%s). Pass --target.' ;;
    zh-CN:unknown_platform) echo '无法识别受支持的平台（%s），请使用 --target 指定。' ;;
    zh-TW:unknown_platform) echo '無法識別受支援的平台（%s），請使用 --target 指定。' ;;
    en:foreign_target) echo 'Note: %s does not match this machine; the files are installed but may not run here.' ;;
    zh-CN:foreign_target) echo '注意：%s 与本机不符，文件会照常安装，但可能无法在本机运行。' ;;
    zh-TW:foreign_target) echo '注意：%s 與本機不符，檔案會照常安裝，但可能無法在本機執行。' ;;
    en:choice) echo 'Choice' ;;
    zh-CN:choice) echo '选择' ;;
    zh-TW:choice) echo '選擇' ;;

    en:pick_edition) echo 'What do you want to install?' ;;
    zh-CN:pick_edition) echo '要安装哪个版本？' ;;
    zh-TW:pick_edition) echo '要安裝哪個版本？' ;;
    en:ed_cli) echo 'CLI: server with the Web console (recommended)' ;;
    zh-CN:ed_cli) echo 'CLI：带 Web 控制台的服务端（推荐）' ;;
    zh-TW:ed_cli) echo 'CLI：帶 Web 控制台的伺服器（建議）' ;;
    en:ed_headless) echo 'Headless CLI: no Web console, for servers and CI' ;;
    zh-CN:ed_headless) echo 'Headless CLI：不含 Web 控制台，适合服务器与 CI' ;;
    zh-TW:ed_headless) echo 'Headless CLI：不含 Web 控制台，適合伺服器與 CI' ;;
    en:ed_app) echo 'Application: desktop / mobile app (opens the download page)' ;;
    zh-CN:ed_app) echo '应用：桌面 / 移动端 App（前往下载页）' ;;
    zh-TW:ed_app) echo '應用程式：桌面 / 行動 App（前往下載頁）' ;;

    en:app_guide) echo 'The Application is installed from its own package. Download it here:' ;;
    zh-CN:app_guide) echo '应用需要单独安装，请在此下载：' ;;
    zh-TW:app_guide) echo '應用程式需要單獨安裝，請在此下載：' ;;
    en:app_open) echo 'Open it in your browser now? (Y/n)' ;;
    zh-CN:app_open) echo '现在在浏览器中打开？(Y/n)' ;;
    zh-TW:app_open) echo '現在在瀏覽器中開啟？(Y/n)' ;;
    en:app_android) echo 'On Android, install the APK from the download page or F-Droid; this script only installs the Termux CLI.' ;;
    zh-CN:app_android) echo 'Android 请从下载页或 F-Droid 安装 APK；本脚本仅安装 Termux CLI。' ;;
    zh-TW:app_android) echo 'Android 請從下載頁或 F-Droid 安裝 APK；本腳本僅安裝 Termux CLI。' ;;
    en:no_headless) echo 'There is no headless build for %s; installing the CLI instead.' ;;
    zh-CN:no_headless) echo '%s 没有 Headless 构建，改为安装 CLI。' ;;
    zh-TW:no_headless) echo '%s 沒有 Headless 建置，改為安裝 CLI。' ;;

    en:pick_mirror) echo 'Download source' ;;
    zh-CN:pick_mirror) echo '下载源' ;;
    zh-TW:pick_mirror) echo '下載來源' ;;
    en:mirror_cnb) echo 'CNB (fastest in mainland China)' ;;
    zh-CN:mirror_cnb) echo 'CNB（中国大陆推荐）' ;;
    zh-TW:mirror_cnb) echo 'CNB（中國大陸建議）' ;;

    en:existing) echo 'Found an existing instance configuration: %s' ;;
    zh-CN:existing) echo '发现已有实例配置：%s' ;;
    zh-TW:existing) echo '發現既有實例設定：%s' ;;
    en:reconfigure) echo 'Reconfigure it? The master key is kept. (y/N)' ;;
    zh-CN:reconfigure) echo '重新配置？主密钥会保留。(y/N)' ;;
    zh-TW:reconfigure) echo '重新設定？主金鑰會保留。(y/N)' ;;
    en:want_configure) echo 'Set up an instance now (database, administrator, proxy)? (Y/n)' ;;
    zh-CN:want_configure) echo '现在配置实例（数据库、管理员、代理）？(Y/n)' ;;
    zh-TW:want_configure) echo '現在設定實例（資料庫、管理員、代理）？(Y/n)' ;;
    en:q_home) echo 'Instance directory' ;;
    zh-CN:q_home) echo '实例目录' ;;
    zh-TW:q_home) echo '實例目錄' ;;
    en:q_db) echo 'Database' ;;
    zh-CN:q_db) echo '数据库' ;;
    zh-TW:q_db) echo '資料庫' ;;
    en:db_sqlite) echo 'SQLite file (recommended for one instance)' ;;
    zh-CN:db_sqlite) echo 'SQLite 文件（单实例推荐）' ;;
    zh-TW:db_sqlite) echo 'SQLite 檔案（單一實例建議）' ;;
    en:db_dsn) echo 'PostgreSQL / MySQL connection string (DSN)' ;;
    zh-CN:db_dsn) echo 'PostgreSQL / MySQL 连接串（DSN）' ;;
    zh-TW:db_dsn) echo 'PostgreSQL / MySQL 連線字串（DSN）' ;;
    en:q_db_path) echo 'SQLite database file' ;;
    zh-CN:q_db_path) echo 'SQLite 数据库文件' ;;
    zh-TW:q_db_path) echo 'SQLite 資料庫檔案' ;;
    en:q_dsn) echo 'DSN (postgres://user:pass@host:5432/db or mysql://...)' ;;
    zh-CN:q_dsn) echo 'DSN（postgres://user:pass@host:5432/db 或 mysql://...）' ;;
    zh-TW:q_dsn) echo 'DSN（postgres://user:pass@host:5432/db 或 mysql://...）' ;;
    en:bad_dsn) echo 'A DSN must start with postgres://, postgresql://, mysql:// or sqlite://.' ;;
    zh-CN:bad_dsn) echo 'DSN 必须以 postgres://、postgresql://、mysql:// 或 sqlite:// 开头。' ;;
    zh-TW:bad_dsn) echo 'DSN 必須以 postgres://、postgresql://、mysql:// 或 sqlite:// 開頭。' ;;
    en:q_host) echo 'Listen address (0.0.0.0 to accept other devices)' ;;
    zh-CN:q_host) echo '监听地址（允许其他设备访问请填 0.0.0.0）' ;;
    zh-TW:q_host) echo '監聽位址（允許其他裝置存取請填 0.0.0.0）' ;;
    en:q_port) echo 'Listen port' ;;
    zh-CN:q_port) echo '监听端口' ;;
    zh-TW:q_port) echo '監聽埠' ;;
    en:bad_port) echo 'The port must be a number from 1 to 65535.' ;;
    zh-CN:bad_port) echo '端口必须是 1 到 65535 之间的数字。' ;;
    zh-TW:bad_port) echo '埠必須是 1 到 65535 之間的數字。' ;;
    en:q_admin) echo 'Administrator username' ;;
    zh-CN:q_admin) echo '管理员用户名' ;;
    zh-TW:q_admin) echo '管理員使用者名稱' ;;
    en:q_password) echo 'Administrator password (Enter to generate one; existing instance: Enter keeps it)' ;;
    zh-CN:q_password) echo '管理员密码（回车自动生成；已有实例回车则保持不变）' ;;
    zh-TW:q_password) echo '管理員密碼（Enter 自動產生；既有實例按 Enter 則維持不變）' ;;
    en:q_password2) echo 'Repeat the password' ;;
    zh-CN:q_password2) echo '再次输入密码' ;;
    zh-TW:q_password2) echo '再次輸入密碼' ;;
    en:password_mismatch) echo 'The passwords do not match.' ;;
    zh-CN:password_mismatch) echo '两次输入的密码不一致。' ;;
    zh-TW:password_mismatch) echo '兩次輸入的密碼不一致。' ;;
    en:password_short) echo 'The password needs at least 8 characters.' ;;
    zh-CN:password_short) echo '密码至少需要 8 个字符。' ;;
    zh-TW:password_short) echo '密碼至少需要 8 個字元。' ;;
    en:q_proxy) echo 'Global upstream proxy (e.g. http://127.0.0.1:7890, socks5h://...; Enter for none)' ;;
    zh-CN:q_proxy) echo '全局上游代理（如 http://127.0.0.1:7890、socks5h://...；回车表示不使用）' ;;
    zh-TW:q_proxy) echo '全域上游代理（如 http://127.0.0.1:7890、socks5h://...；Enter 表示不使用）' ;;
    en:bad_proxy) echo 'Proxy URLs start with http://, https://, socks5:// or socks5h://.' ;;
    zh-CN:bad_proxy) echo '代理地址须以 http://、https://、socks5:// 或 socks5h:// 开头。' ;;
    zh-TW:bad_proxy) echo '代理位址須以 http://、https://、socks5:// 或 socks5h:// 開頭。' ;;
    en:bad_quote) echo 'Values cannot contain quotes, backslashes or newlines.' ;;
    zh-CN:bad_quote) echo '取值不能包含引号、反斜杠或换行。' ;;
    zh-TW:bad_quote) echo '取值不能包含引號、反斜線或換行。' ;;
    en:q_service) echo 'Run GPROXY as a background service that starts automatically? (y/N)' ;;
    zh-CN:q_service) echo '注册为开机/登录自动启动的后台服务？(y/N)' ;;
    zh-TW:q_service) echo '註冊為開機/登入自動啟動的背景服務？(y/N)' ;;

    en:summary) echo 'About to install:' ;;
    zh-CN:summary) echo '即将安装：' ;;
    zh-TW:summary) echo '即將安裝：' ;;
    en:s_edition) echo 'Edition' ;;
    zh-CN:s_edition) echo '版本' ;;
    zh-TW:s_edition) echo '版本' ;;
    en:s_platform) echo 'Platform' ;;
    zh-CN:s_platform) echo '平台' ;;
    zh-TW:s_platform) echo '平台' ;;
    en:s_channel) echo 'Channel' ;;
    zh-CN:s_channel) echo '通道' ;;
    zh-TW:s_channel) echo '通道' ;;
    en:s_mirror) echo 'Source' ;;
    zh-CN:s_mirror) echo '下载源' ;;
    zh-TW:s_mirror) echo '下載來源' ;;
    en:s_dir) echo 'Command' ;;
    zh-CN:s_dir) echo '命令' ;;
    zh-TW:s_dir) echo '指令' ;;
    en:s_apt) echo 'Termux package (apt)' ;;
    zh-CN:s_apt) echo 'Termux 软件包（apt）' ;;
    zh-TW:s_apt) echo 'Termux 套件（apt）' ;;
    en:s_home) echo 'Instance' ;;
    zh-CN:s_home) echo '实例' ;;
    zh-TW:s_home) echo '實例' ;;
    en:s_db) echo 'Database' ;;
    zh-CN:s_db) echo '数据库' ;;
    zh-TW:s_db) echo '資料庫' ;;
    en:s_listen) echo 'Listen' ;;
    zh-CN:s_listen) echo '监听' ;;
    zh-TW:s_listen) echo '監聽' ;;
    en:s_admin) echo 'Admin' ;;
    zh-CN:s_admin) echo '管理员' ;;
    zh-TW:s_admin) echo '管理員' ;;
    en:s_proxy) echo 'Proxy' ;;
    zh-CN:s_proxy) echo '代理' ;;
    zh-TW:s_proxy) echo '代理' ;;
    en:s_service) echo 'Service' ;;
    zh-CN:s_service) echo '服务' ;;
    zh-TW:s_service) echo '服務' ;;
    en:s_keep) echo 'keep existing configuration' ;;
    zh-CN:s_keep) echo '保留现有配置' ;;
    zh-TW:s_keep) echo '保留現有設定' ;;
    en:s_none) echo 'none' ;;
    zh-CN:s_none) echo '无' ;;
    zh-TW:s_none) echo '無' ;;
    en:s_yes) echo 'yes' ;;
    zh-CN:s_yes) echo '是' ;;
    zh-TW:s_yes) echo '是' ;;
    en:s_no) echo 'no' ;;
    zh-CN:s_no) echo '否' ;;
    zh-TW:s_no) echo '否' ;;
    en:s_generated) echo 'generated' ;;
    zh-CN:s_generated) echo '自动生成' ;;
    zh-TW:s_generated) echo '自動產生' ;;
    en:s_set) echo 'set' ;;
    zh-CN:s_set) echo '已设置' ;;
    zh-TW:s_set) echo '已設定' ;;
    en:proceed) echo 'Continue? (Y/n)' ;;
    zh-CN:proceed) echo '继续？(Y/n)' ;;
    zh-TW:proceed) echo '繼續？(Y/n)' ;;
    en:aborted) echo 'Cancelled.' ;;
    zh-CN:aborted) echo '已取消。' ;;
    zh-TW:aborted) echo '已取消。' ;;

    en:resolving) echo 'Looking up the latest %s version...' ;;
    zh-CN:resolving) echo '正在查询 %s 通道的最新版本……' ;;
    zh-TW:resolving) echo '正在查詢 %s 通道的最新版本……' ;;
    en:downloading) echo 'Downloading %s' ;;
    zh-CN:downloading) echo '正在下载 %s' ;;
    zh-TW:downloading) echo '正在下載 %s' ;;
    en:verified) echo 'SHA-256 verified.' ;;
    zh-CN:verified) echo 'SHA-256 校验通过。' ;;
    zh-TW:verified) echo 'SHA-256 驗證通過。' ;;
    en:bad_sum) echo 'Checksum mismatch for %s; nothing was installed.' ;;
    zh-CN:bad_sum) echo '%s 校验和不匹配，未安装任何文件。' ;;
    zh-TW:bad_sum) echo '%s 校驗和不符，未安裝任何檔案。' ;;
    en:no_sum) echo 'No SHA-256 entry for %s in SHA256SUMS.' ;;
    zh-CN:no_sum) echo 'SHA256SUMS 中没有 %s 的条目。' ;;
    zh-TW:no_sum) echo 'SHA256SUMS 中沒有 %s 的條目。' ;;
    en:need) echo 'Required command not found: %s' ;;
    zh-CN:need) echo '缺少必需的命令：%s' ;;
    zh-TW:need) echo '缺少必要的指令：%s' ;;
    en:no_unzip) echo 'Need unzip, bsdtar, busybox or python3 to extract the ZIP.' ;;
    zh-CN:no_unzip) echo '需要 unzip、bsdtar、busybox 或 python3 之一来解压 ZIP。' ;;
    zh-TW:no_unzip) echo '需要 unzip、bsdtar、busybox 或 python3 之一來解壓 ZIP。' ;;
    en:need_termux) echo 'Android builds run only inside Termux.' ;;
    zh-CN:need_termux) echo 'Android 构建只能在 Termux 中运行。' ;;
    zh-TW:need_termux) echo 'Android 建置只能在 Termux 中執行。' ;;
    en:installed) echo 'Installed GPROXY %s: %s' ;;
    zh-CN:installed) echo '已安装 GPROXY %s：%s' ;;
    zh-TW:installed) echo '已安裝 GPROXY %s：%s' ;;

    en:writing_env) echo 'Writing %s' ;;
    zh-CN:writing_env) echo '正在写入 %s' ;;
    zh-TW:writing_env) echo '正在寫入 %s' ;;
    en:bootstrapping) echo 'Preparing the database and administrator...' ;;
    zh-CN:bootstrapping) echo '正在初始化数据库与管理员……' ;;
    zh-TW:bootstrapping) echo '正在初始化資料庫與管理員……' ;;
    en:setting_proxy) echo 'Setting the global proxy...' ;;
    zh-CN:setting_proxy) echo '正在设置全局代理……' ;;
    zh-TW:setting_proxy) echo '正在設定全域代理……' ;;
    en:proxy_failed) echo 'Could not set the proxy; set it later in Console > Settings.' ;;
    zh-CN:proxy_failed) echo '代理设置失败，请稍后在 控制台 > 设置 中配置。' ;;
    zh-TW:proxy_failed) echo '代理設定失敗，請稍後在 控制台 > 設定 中設定。' ;;
    en:installing_service) echo 'Registering the background service...' ;;
    zh-CN:installing_service) echo '正在注册后台服务……' ;;
    zh-TW:installing_service) echo '正在註冊背景服務……' ;;
    en:service_failed) echo 'Service registration failed; start GPROXY manually with the command below.' ;;
    zh-CN:service_failed) echo '服务注册失败，请用下面的命令手动启动。' ;;
    zh-TW:service_failed) echo '服務註冊失敗，請以下方指令手動啟動。' ;;
    en:restarting) echo 'Restarting the running service...' ;;
    zh-CN:restarting) echo '正在重启运行中的服务……' ;;
    zh-TW:restarting) echo '正在重新啟動執行中的服務……' ;;

    en:not_in_path) echo '%s is not on your PATH. Add this line to your shell profile:' ;;
    zh-CN:not_in_path) echo '%s 不在 PATH 中，请把下面这行加入 shell 配置文件：' ;;
    zh-TW:not_in_path) echo '%s 不在 PATH 中，請把下面這行加入 shell 設定檔：' ;;
    en:next) echo 'Start the server:' ;;
    zh-CN:next) echo '启动服务：' ;;
    zh-TW:next) echo '啟動服務：' ;;
    en:running) echo 'GPROXY is running as a service: %s' ;;
    zh-CN:running) echo 'GPROXY 已作为服务运行：%s' ;;
    zh-TW:running) echo 'GPROXY 已作為服務執行：%s' ;;
    en:console_at) echo 'Console: %s' ;;
    zh-CN:console_at) echo '控制台：%s' ;;
    zh-TW:console_at) echo '控制台：%s' ;;
    en:save_creds) echo 'Save the administrator credentials printed above; they are shown only once.' ;;
    zh-CN:save_creds) echo '请保存上面打印的管理员凭据，它们只显示一次。' ;;
    zh-TW:save_creds) echo '請保存上方列印的管理員憑證，它們只會顯示一次。' ;;
    en:keep_env) echo 'Back up %s: it holds the master key that decrypts stored credentials.' ;;
    zh-CN:keep_env) echo '请备份 %s：其中的主密钥用于解密已存储的凭证。' ;;
    zh-TW:keep_env) echo '請備份 %s：其中的主金鑰用於解密已儲存的憑證。' ;;
    en:next_plain) echo 'First start prints a generated administrator password and API key once; save them.' ;;
    zh-CN:next_plain) echo '首次启动会打印一次自动生成的管理员密码与 API 密钥，请妥善保存。' ;;
    zh-TW:next_plain) echo '首次啟動會列印一次自動產生的管理員密碼與 API 金鑰，請妥善保存。' ;;
    en:docs) echo 'Docs' ;;
    zh-CN:docs) echo '文档' ;;
    zh-TW:docs) echo '文件' ;;
    en:app_hint) echo 'Prefer a desktop or mobile app? Download it from' ;;
    zh-CN:app_hint) echo '想用桌面或移动端 App？请前往' ;;
    zh-TW:app_hint) echo '想用桌面或行動 App？請前往' ;;
  esac
}

say() { key="$1"; shift; printf "$(msg "$key")\n" "$@"; }
warn() { key="$1"; shift; printf "$(msg "$key")\n" "$@" >&2; }
die() { warn "$@"; exit 1; }
yes_answer() { case "$1" in n|N|no|NO|No|否|不) return 1 ;; *) return 0 ;; esac; }
no_answer() { case "$1" in y|Y|yes|YES|Yes|是|好) return 1 ;; *) return 0 ;; esac; }

[ -z "$lang" ] || lang="$(normalize_lang "$lang")"
if [ -z "$lang" ]; then
  lang="$(detect_lang)"
  if [ "$interactive" -eq 1 ]; then
    case "$lang" in en) default=1 ;; zh-CN) default=2 ;; *) default=3 ;; esac
    printf 'Language / 语言 / 語言\n  1) English\n  2) 简体中文\n  3) 繁體中文\n' >/dev/tty
    ask choice '>' "$default"
    lang="$(normalize_lang "$choice")"
    [ -n "$lang" ] || lang="$(normalize_lang "$default")"
  fi
fi

case "$lang" in
  zh-CN) docs_base="$SITE/zh-cn" ;;
  zh-TW) docs_base="$SITE/zh-tw" ;;
  *) docs_base="$SITE" ;;
esac
download_page="$docs_base/download/"

# ---------------------------------------------------------------- platform

termux=0
case "${PREFIX:-}" in */com.termux/*) termux=1 ;; esac

is_musl() {
  ls /lib/ld-musl-* >/dev/null 2>&1 && return 0
  ldd --version 2>&1 | grep -qi musl
}

detect_target() {
  os="$(uname -s 2>/dev/null || echo unknown)"
  arch="$(uname -m 2>/dev/null || echo unknown)"
  case "$arch" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64|armv8*) arch=aarch64 ;;
    riscv64) arch=riscv64 ;;
    *) echo "unsupported:$os/$arch"; return ;;
  esac
  case "$os" in
    Darwin)
      # Rosetta reports x86_64 for translated shells on Apple Silicon.
      if [ "$arch" = x86_64 ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = 1 ]; then
        arch=aarch64
      fi
      echo "macos-$arch" ;;
    Linux)
      if [ "$termux" -eq 1 ] || [ "$(uname -o 2>/dev/null)" = Android ]; then
        echo "android-$arch"
      elif uname -a 2>/dev/null | grep -qiE 'ohos|openharmony|harmonyos'; then
        echo "ohos-$arch"
      elif is_musl; then
        echo "linux-$arch-musl"
      else
        echo "linux-$arch"
      fi ;;
    HarmonyOS|OpenHarmony|OHOS) echo "ohos-$arch" ;;
    *) echo "unsupported:$os/$arch" ;;
  esac
}

all_targets='linux-x86_64 linux-aarch64 linux-riscv64 linux-x86_64-musl linux-aarch64-musl linux-riscv64-musl macos-x86_64 macos-aarch64 android-x86_64 android-aarch64 ohos-x86_64 ohos-aarch64'

valid_target() {
  for t in $all_targets; do [ "$t" = "$1" ] && return 0; done
  return 1
}

detected="$(detect_target)"
if [ -z "$target" ]; then
  case "$detected" in unsupported:*) ;; *) target="$detected" ;; esac
  if [ "$interactive" -eq 1 ]; then
    keep=n
    if [ -n "$target" ]; then
      say detected "$target" >/dev/tty
      ask keep "$(msg keep_target)" ''
      [ -n "$keep" ] || keep=y
    fi
    if ! yes_answer "$keep" || [ -z "$target" ]; then
      printf '%s\n' "$(msg pick_target)" >/dev/tty
      i=0
      for t in $all_targets; do i=$((i + 1)); printf '  %2d) %s\n' "$i" "$t" >/dev/tty; done
      ask choice "$(msg choice)" ''
      i=0
      for t in $all_targets; do
        i=$((i + 1))
        if [ "$choice" = "$i" ] || [ "$choice" = "$t" ]; then target="$t"; fi
      done
    fi
  fi
fi
[ -n "$target" ] || die unknown_platform "${detected#unsupported:}"
valid_target "$target" || die unknown_platform "$target"
case "$detected" in
  unsupported:*|"$target") ;;
  *) say foreign_target "$target" ;;
esac

# ---------------------------------------------------------------- edition

open_url() {
  for opener in xdg-open open termux-open-url; do
    if command -v "$opener" >/dev/null 2>&1; then
      "$opener" "$1" >/dev/null 2>&1 &
      return 0
    fi
  done
  return 1
}

if [ -z "$edition" ]; then
  edition=cli
  if [ "$interactive" -eq 1 ]; then
    printf '%s\n  1) %s\n  2) %s\n  3) %s\n' "$(msg pick_edition)" \
      "$(msg ed_cli)" "$(msg ed_headless)" "$(msg ed_app)" >/dev/tty
    ask choice "$(msg choice)" 1
    case "$choice" in 2|headless) edition=headless ;; 3|app) edition=app ;; esac
  fi
fi

case "$edition" in
  cli|headless) ;;
  app|application|desktop)
    say app_guide
    printf '  %s\n' "$download_page"
    case "$target" in android-*) say app_android ;; esac
    if [ "$interactive" -eq 1 ]; then
      ask choice "$(msg app_open)" ''
      if yes_answer "$choice"; then open_url "$download_page" || true; fi
    fi
    exit 0 ;;
  *) echo "unknown edition: $edition" >&2; exit 2 ;;
esac

if [ "$edition" = headless ]; then
  case "$target" in ohos-*) say no_headless "$target"; edition=cli ;; esac
fi

# ---------------------------------------------------------------- source

case "$channel" in
  release|stable) channel=release ;;
  beta|staging) channel=beta ;;
  dev|nightly) channel=dev ;;
  *) echo "unknown channel: $channel" >&2; exit 2 ;;
esac

if [ -z "$mirror" ]; then
  if [ "$lang" = zh-CN ]; then mirror=cnb; else mirror=github; fi
  if [ "$interactive" -eq 1 ]; then
    case "$mirror" in cnb) default=3 ;; *) default=1 ;; esac
    printf '%s\n  1) GitHub\n  2) GitLab\n  3) %s\n' "$(msg pick_mirror)" "$(msg mirror_cnb)" >/dev/tty
    ask choice "$(msg choice)" "$default"
    case "$choice" in 1|github) mirror=github ;; 2|gitlab) mirror=gitlab ;; 3|cnb) mirror=cnb ;; esac
  fi
fi

# asset_url TAG NAME
case "$mirror" in
  github) asset_url() { echo "https://github.com/LeenHawk/gproxy/releases/download/$1/$2"; } ;;
  gitlab) asset_url() { echo "https://gitlab.com/leenhawk1/gproxy/-/releases/$1/downloads/$2"; } ;;
  cnb) asset_url() { echo "https://cnb.cool/LeenHawk/gproxy/-/releases/download/$1/$2"; } ;;
  *) echo "unknown mirror: $mirror" >&2; exit 2 ;;
esac

# ---------------------------------------------------------------- locations

use_apt=0
case "$target" in
  android-*)
    [ "$termux" -eq 1 ] || die need_termux
    use_apt=1 ;;
esac

if [ -z "$bindir" ] && [ "$use_apt" -eq 0 ]; then
  if [ "$(id -u 2>/dev/null || echo 1)" = 0 ]; then bindir=/usr/local/bin; else bindir="$HOME/.local/bin"; fi
fi
# OHOS needs its bundled libc++ beside the binary; keep the whole archive.
[ "$use_apt" -eq 1 ] || libdir="$(dirname "$bindir")/lib/gproxy"

# ---------------------------------------------------------------- instance

[ -n "$home_dir" ] || home_dir="${XDG_DATA_HOME:-$HOME/.local/share}/gproxy"
env_file="$home_dir/.env"

env_value() { # env_value NAME: a value from the existing .env, unquoted
  [ -f "$env_file" ] || return 0
  sed -n "s/^[[:space:]]*\(export[[:space:]]\{1,\}\)\{0,1\}$1=//p" "$env_file" | tail -n 1 |
    sed "s/^'\(.*\)'\$/\1/; s/^\"\(.*\)\"\$/\1/"
}

# Reconfiguring starts from what the existing .env says, so changing one
# setting does not reset the others.
load_current() {
  cur_host="$(env_value GPROXY_HOST)"
  cur_port="$(env_value GPROXY_PORT)"
  cur_dsn=''
  cur_db=''
  current="$(env_value GPROXY_DSN)"
  case "$current" in
    '') ;;
    postgres://*|postgresql://*|mysql://*) cur_dsn="$current" ;;
    *) cur_db="$current" ;;
  esac
  [ -n "$cur_db" ] || cur_db="$home_dir/gproxy.db"
}
load_current

# Anything named on the command line means "configure", even without -y.
if [ -n "$db_path$dsn$listen_host$listen_port$admin_user$admin_password$proxy$service" ]; then
  [ "$configure" = 0 ] || configure=1
fi

reconfigure=1
if [ -z "$configure" ]; then
  if [ -f "$env_file" ]; then
    reconfigure=0
    if [ "$interactive" -eq 1 ]; then
      say existing "$env_file" >/dev/tty
      ask choice "$(msg reconfigure)" ''
      no_answer "$choice" || reconfigure=1
    fi
    configure=1
  elif [ "$interactive" -eq 1 ]; then
    ask choice "$(msg want_configure)" ''
    if yes_answer "$choice"; then configure=1; else configure=0; fi
  else
    configure=1
  fi
fi

valid_value() {
  case "$1" in *"'"*|*'"'*|*'\'*) return 1 ;; esac
  [ "$(printf '%s' "$1" | wc -l)" -eq 0 ]
}

valid_port() {
  case "$1" in ''|*[!0-9]*) return 1 ;; esac
  [ "$1" -ge 1 ] && [ "$1" -le 65535 ]
}

valid_dsn() {
  case "$1" in postgres://*|postgresql://*|mysql://*|sqlite://*) return 0 ;; *) return 1 ;; esac
}

valid_proxy() {
  case "$1" in http://?*|https://?*|socks5://?*|socks5h://?*) valid_value "$1" ;; *) return 1 ;; esac
}

if [ "$configure" = 1 ] && [ "$reconfigure" = 1 ] && [ "$interactive" -eq 1 ]; then
  while :; do
    ask home_dir "$(msg q_home)" "$home_dir"
    valid_value "$home_dir" && break
    warn bad_quote
  done
  env_file="$home_dir/.env"
  load_current
  if [ -z "$db_path$dsn" ]; then
    printf '%s\n  1) %s\n  2) %s\n' "$(msg q_db)" "$(msg db_sqlite)" "$(msg db_dsn)" >/dev/tty
    if [ -n "$cur_dsn" ]; then default=2; else default=1; fi
    ask choice "$(msg choice)" "$default"
    if [ "$choice" = 2 ]; then
      while :; do
        ask dsn "$(msg q_dsn)" "$cur_dsn"
        valid_dsn "$dsn" && valid_value "$dsn" && break
        warn bad_dsn
      done
    else
      while :; do
        ask db_path "$(msg q_db_path)" "$cur_db"
        valid_value "$db_path" && break
        warn bad_quote
      done
    fi
  fi
  [ -n "$listen_host" ] || ask listen_host "$(msg q_host)" "${cur_host:-127.0.0.1}"
  while [ -z "$listen_port" ] || ! valid_port "$listen_port"; do
    ask listen_port "$(msg q_port)" "${cur_port:-8787}"
    valid_port "$listen_port" || { warn bad_port; listen_port=''; }
  done
  [ -n "$admin_user" ] || ask admin_user "$(msg q_admin)" admin
  while [ -z "$admin_password" ]; do
    ask_secret first "$(msg q_password)"
    [ -n "$first" ] || break
    if [ "${#first}" -lt 8 ]; then warn password_short; continue; fi
    ask_secret second "$(msg q_password2)"
    if [ "$first" = "$second" ]; then admin_password="$first"; else warn password_mismatch; fi
  done
  while [ -z "$proxy" ]; do
    ask proxy "$(msg q_proxy)" ''
    [ -n "$proxy" ] || break
    valid_proxy "$proxy" || { warn bad_proxy; proxy=''; }
  done
  case "$target" in ohos-*) service=0 ;; esac
  if [ -z "$service" ]; then
    ask choice "$(msg q_service)" ''
    if no_answer "$choice"; then service=0; else service=1; fi
  fi
fi

[ -n "$listen_host" ] || listen_host="${cur_host:-127.0.0.1}"
[ -n "$listen_port" ] || listen_port="${cur_port:-8787}"
[ -n "$db_path$dsn" ] || { dsn="$cur_dsn"; [ -n "$dsn" ] || db_path="$cur_db"; }
[ -n "$admin_user" ] || admin_user=admin
[ -n "$service" ] || service=0
case "$target" in ohos-*) service=0 ;; esac
valid_port "$listen_port" || die bad_port
[ -z "$dsn" ] || valid_dsn "$dsn" || die bad_dsn
[ -z "$proxy" ] || valid_proxy "$proxy" || die bad_proxy
[ -z "$admin_password" ] || [ "${#admin_password}" -ge 8 ] || die password_short
for value in "$home_dir" "$db_path" "$dsn" "$listen_host" "$admin_user"; do
  valid_value "$value" || die bad_quote
done

# ---------------------------------------------------------------- summary

prefix='gproxy'
[ "$edition" = headless ] && prefix='gproxy-headless'
if [ "$use_apt" -eq 1 ]; then asset="$prefix-$target.deb"; else asset="$prefix-$target.zip"; fi

yes_no() { if [ "$1" = 1 ]; then msg s_yes; else msg s_no; fi; }
row() { printf '  - %s: %s\n' "$(msg "$1")" "$2"; }

say summary
row s_edition "$edition"
row s_platform "$target"
row s_channel "$channel"
row s_mirror "$mirror"
if [ "$use_apt" -eq 1 ]; then row s_dir "$(msg s_apt)"; else row s_dir "$bindir/gproxy"; fi
if [ "$configure" = 1 ]; then
  row s_home "$home_dir"
  if [ "$reconfigure" = 1 ]; then
    if [ -n "$dsn" ]; then row s_db "${dsn%%://*}://…"; else row s_db "$db_path"; fi
    row s_listen "$listen_host:$listen_port"
    if [ -n "$admin_password" ]; then row s_admin "$admin_user ($(msg s_set))"; else row s_admin "$admin_user ($(msg s_generated))"; fi
    row s_proxy "${proxy:-$(msg s_none)}"
    row s_service "$(yes_no "$service")"
  else
    row s_db "$(msg s_keep)"
  fi
fi
if [ "$interactive" -eq 1 ]; then
  ask choice "$(msg proceed)" ''
  yes_answer "$choice" || { say aborted; exit 0; }
fi

# ---------------------------------------------------------------- download

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }
  fetch_progress() { curl -fL --retry 3 --progress-bar -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q -O "$2" "$1"; }
  fetch_progress() { wget -q --show-progress -O "$2" "$1" 2>/dev/null || wget -q -O "$2" "$1"; }
else
  die need 'curl / wget'
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
elif command -v openssl >/dev/null 2>&1; then
  sha256() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }
else
  die need 'sha256sum / shasum / openssl'
fi

work="$(mktemp -d 2>/dev/null || mktemp -d -t gproxy)"
trap 'rm -rf "$work"' EXIT
trap 'stty echo </dev/tty 2>/dev/null; exit 130' INT TERM

# Resolve the channel to a concrete tag through its manifest, so the package
# and SHA256SUMS always come from the same release.
say resolving "$channel"
case "$channel" in
  release)
    if [ "$mirror" = github ]; then
      manifest_url='https://github.com/LeenHawk/gproxy/releases/latest/download/manifest.json'
    else
      manifest_url="$(asset_url release manifest.json)"
    fi ;;
  beta) manifest_url="$(asset_url staging manifest.json)" ;;
  dev) manifest_url="$(asset_url nightly manifest.json)" ;;
esac
fetch "$manifest_url" "$work/manifest.json"
version="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$work/manifest.json" | head -n 1)"
[ -n "$version" ] || { echo "manifest.json has no version: $manifest_url" >&2; exit 1; }
case "$channel" in
  release) tag="v$version" ;;
  beta) tag=staging ;;
  dev) tag=nightly ;;
esac

fetch "$(asset_url "$tag" SHA256SUMS)" "$work/SHA256SUMS"
say downloading "$asset ($version)"
fetch_progress "$(asset_url "$tag" "$asset")" "$work/$asset"

expected="$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1; exit }' "$work/SHA256SUMS")"
[ -n "$expected" ] || die no_sum "$asset"
[ "$(sha256 "$work/$asset")" = "$expected" ] || die bad_sum "$asset"
say verified

# ---------------------------------------------------------------- install

extract() {
  mkdir -p "$2"
  if command -v unzip >/dev/null 2>&1; then
    unzip -qo "$1" -d "$2"
  elif command -v bsdtar >/dev/null 2>&1; then
    bsdtar -xf "$1" -C "$2"
  elif command -v busybox >/dev/null 2>&1; then
    busybox unzip -qo "$1" -d "$2"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -I -m zipfile -e "$1" "$2"
  else
    die no_unzip
  fi
}

if [ "$use_apt" -eq 1 ]; then
  # apt resolves libc++, openssl and ca-certificates; the full and headless
  # packages conflict, so apt replaces whichever is installed.
  apt install -y "$work/$asset"
  gproxy="${PREFIX}/bin/gproxy"
else
  extract "$work/$asset" "$work/out"
  mkdir -p "$bindir"
  case "$target" in
    ohos-*)
      rm -rf "$libdir.new"
      mkdir -p "$libdir.new"
      cp -R "$work/out/." "$libdir.new/"
      chmod +x "$libdir.new/gproxy" "$libdir.new/run-gproxy.sh"
      rm -rf "$libdir"
      mv "$libdir.new" "$libdir"
      printf '#!/bin/sh\nexec sh "%s/run-gproxy.sh" "$@"\n' "$libdir" > "$work/wrapper"
      chmod +x "$work/wrapper"
      mv -f "$work/wrapper" "$bindir/gproxy" ;;
    *)
      chmod +x "$work/out/gproxy"
      # Copy beside the destination first so the final rename is atomic and a
      # running gproxy keeps its old inode.
      cp "$work/out/gproxy" "$bindir/.gproxy.new"
      mv -f "$bindir/.gproxy.new" "$bindir/gproxy" ;;
  esac
  gproxy="$bindir/gproxy"
fi

echo
say installed "$version" "$gproxy"

# ---------------------------------------------------------------- configure

random_hex() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    od -An -tx1 -N32 /dev/urandom | tr -d ' \n'
  fi
}

# Every gproxy step runs in the instance directory, where it reads .env.
in_home() { (cd "$home_dir" && GPROXY_LOG_FILTER=warn "$@"); }

# Write settings.instance.proxy through a settings-only import, so nothing
# else in the instance (providers, credentials) is rewritten.
set_proxy() {
  in_home "$gproxy" export --out "$work/export.json" || return 1
  awk -v url="$1" '
    !started { print; if ($0 ~ /^  "data": \{$/) started = 1; next }
    !settings { if ($0 ~ /^    "settings": /) settings = 1; else next }
    skipping { if ($0 ~ /^ *\},?$/) skipping = 0; next }
    /^ *"proxy": / {
      indent = $0; sub(/[^ ].*/, "", indent)
      if ($0 ~ /\{ *$/) skipping = 1
      print indent "\"proxy\": {\"mode\": \"explicit\", \"url\": \"" url "\"},"
      found = 1
      next
    }
    { print }
    END { if (!found) exit 1 }' "$work/export.json" > "$work/settings.json" || return 1
  in_home "$gproxy" import --in "$work/settings.json" >/dev/null
}

service_name=''
case "$target" in
  linux-*) service_name='systemctl --user status gproxy' ;;
  macos-*) service_name="launchctl print gui/$(id -u)/com.leenhawk.gproxy.cli" ;;
  android-*) service_name='~/.termux/boot/gproxy.sh' ;;
esac

restart_service() {
  case "$target" in
    linux-*)
      if command -v systemctl >/dev/null 2>&1 && systemctl --user is-active --quiet gproxy.service 2>/dev/null; then
        say restarting
        systemctl --user restart gproxy.service || true
      fi ;;
    macos-*)
      if launchctl print "gui/$(id -u)/com.leenhawk.gproxy.cli" >/dev/null 2>&1; then
        say restarting
        launchctl kickstart -k "gui/$(id -u)/com.leenhawk.gproxy.cli" || true
      fi ;;
  esac
}

service_running=0
if [ "$configure" = 1 ] && [ "$reconfigure" = 1 ]; then
  mkdir -p "$home_dir"
  chmod 700 "$home_dir" 2>/dev/null || true
  master_key="$(env_value GPROXY_MASTER_KEY)"
  [ -n "$master_key" ] || master_key="$(random_hex)"
  update_source="$mirror"
  say writing_env "$env_file"
  umask 077
  {
    echo '# Written by the GPROXY installer. Read by `gproxy` started in this directory'
    echo '# and by the service. Keep GPROXY_MASTER_KEY: it decrypts stored credentials.'
    echo "GPROXY_HOST='$listen_host'"
    echo "GPROXY_PORT='$listen_port'"
    echo "GPROXY_DATA_DIR='$home_dir'"
    echo "GPROXY_DSN='${dsn:-$db_path}'"
    echo "GPROXY_MASTER_KEY='$master_key'"
    echo "GPROXY_UPDATE_CHANNEL='$channel'"
    echo "GPROXY_UPDATE_SOURCE='$update_source'"
  } > "$env_file.new"
  mv -f "$env_file.new" "$env_file"
  chmod 600 "$env_file"
  umask 022
  [ -z "$db_path" ] || mkdir -p "$(dirname "$db_path")"

  say bootstrapping
  (
    export GPROXY_ADMIN_USER="$admin_user"
    if [ -n "$admin_password" ]; then export GPROXY_ADMIN_PASSWORD="$admin_password"; fi
    in_home "$gproxy" bootstrap admin
  )

  if [ -n "$proxy" ]; then
    say setting_proxy
    set_proxy "$proxy" || warn proxy_failed
  fi

  restart_service
  if [ "$service" = 1 ]; then
    say installing_service
    if in_home "$gproxy" service install; then service_running=1; else warn service_failed; fi
  fi
elif [ "$configure" = 1 ]; then
  restart_service
fi

# ---------------------------------------------------------------- next steps

echo
if [ "$use_apt" -eq 0 ]; then
  case ":$PATH:" in
    *":$bindir:"*) ;;
    *)
      say not_in_path "$bindir"
      printf '  export PATH="%s:$PATH"\n\n' "$bindir" ;;
  esac
fi

if [ "$configure" = 1 ]; then
  if [ "$service_running" = 1 ]; then
    say running "$service_name"
  else
    say next
    printf '  cd "%s" && gproxy serve\n' "$home_dir"
  fi
  if [ "$reconfigure" = 1 ]; then
    if [ "$edition" = cli ]; then
      case "$listen_host" in 0.0.0.0|::|'[::]') console_host=127.0.0.1 ;; *) console_host="$listen_host" ;; esac
      say console_at "http://$console_host:$listen_port/console/"
    fi
    [ -n "$admin_password" ] || say save_creds
    say keep_env "$env_file"
  fi
else
  say next
  printf '  gproxy serve --data-dir "%s"\n' "$home_dir"
  say next_plain
fi
printf '%s: %s/getting-started/quick-start/\n' "$(msg docs)" "$docs_base"
printf '%s %s\n' "$(msg app_hint)" "$download_page"
