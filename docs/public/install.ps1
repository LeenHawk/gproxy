# GPROXY CLI installer for Windows (x64, ARM64).
#
#   irm https://gproxy.leenhawk.com/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://gproxy.leenhawk.com/install.ps1))) -Edition headless -Yes
#
# Linux, macOS, Termux: curl -fsSL https://gproxy.leenhawk.com/install.sh | sh
# The desktop Application is not installed by this script; choose it in the
# menu to be pointed at Microsoft Store or the download page.
#
# This file is kept ASCII so Windows PowerShell 5.1 reads it the same way from
# disk and through `irm`; localized messages are \u escapes, decoded at runtime.
[CmdletBinding()]
param(
    [string]$Edition = '',
    [string]$Target = '',
    [string]$Channel = 'release',
    [string]$Mirror = '',
    [string]$InstallDir = '',
    [string]$DataDir = '',
    [string]$DbPath = '',
    [string]$Dsn = '',
    [string]$ListenHost = '',
    [string]$Port = '',
    [string]$AdminUser = '',
    [string]$AdminPassword = $env:GPROXY_ADMIN_PASSWORD,
    [string]$Proxy = '',
    [switch]$Service,
    [switch]$NoService,
    [switch]$NoConfigure,
    [string]$Lang = '',
    [switch]$Yes
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { }
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch { }

$Site = 'https://gproxy.leenhawk.com'
$StoreDesktop = '9P2FJRB9RS4Z'

# ---------------------------------------------------------------- messages

# key = English, Simplified Chinese, Traditional Chinese. Placeholders are -f's.
$Messages = @{
    detected           = @('Detected platform: {0}', '\u68c0\u6d4b\u5230\u7684\u5e73\u53f0\uff1a{0}', '\u5075\u6e2c\u5230\u7684\u5e73\u53f0\uff1a{0}')
    keep_target        = @('Use this platform? (Y/n)', '\u4f7f\u7528\u6b64\u5e73\u53f0\uff1f(Y/n)', '\u4f7f\u7528\u6b64\u5e73\u53f0\uff1f(Y/n)')
    pick_target        = @('Choose a platform', '\u9009\u62e9\u5e73\u53f0', '\u9078\u64c7\u5e73\u53f0')
    foreign_target     = @('Note: {0} does not match this machine; the files are installed but may not run here.', '\u6ce8\u610f\uff1a{0} \u4e0e\u672c\u673a\u4e0d\u7b26\uff0c\u6587\u4ef6\u4f1a\u7167\u5e38\u5b89\u88c5\uff0c\u4f46\u53ef\u80fd\u65e0\u6cd5\u5728\u672c\u673a\u8fd0\u884c\u3002', '\u6ce8\u610f\uff1a{0} \u8207\u672c\u6a5f\u4e0d\u7b26\uff0c\u6a94\u6848\u6703\u7167\u5e38\u5b89\u88dd\uff0c\u4f46\u53ef\u80fd\u7121\u6cd5\u5728\u672c\u6a5f\u57f7\u884c\u3002')
    choice             = @('Choice', '\u9009\u62e9', '\u9078\u64c7')
    pick_edition       = @('What do you want to install?', '\u8981\u5b89\u88c5\u54ea\u4e2a\u7248\u672c\uff1f', '\u8981\u5b89\u88dd\u54ea\u500b\u7248\u672c\uff1f')
    ed_cli             = @('CLI: server with the Web console (recommended)', 'CLI\uff1a\u5e26 Web \u63a7\u5236\u53f0\u7684\u670d\u52a1\u7aef\uff08\u63a8\u8350\uff09', 'CLI\uff1a\u5e36 Web \u63a7\u5236\u53f0\u7684\u4f3a\u670d\u5668\uff08\u5efa\u8b70\uff09')
    ed_headless        = @('Headless CLI: no Web console, for servers and CI', 'Headless CLI\uff1a\u4e0d\u542b Web \u63a7\u5236\u53f0\uff0c\u9002\u5408\u670d\u52a1\u5668\u4e0e CI', 'Headless CLI\uff1a\u4e0d\u542b Web \u63a7\u5236\u53f0\uff0c\u9069\u5408\u4f3a\u670d\u5668\u8207 CI')
    ed_app             = @('Application: GPROXY Desktop (Microsoft Store or download page)', '\u5e94\u7528\uff1aGPROXY Desktop\uff08Microsoft Store \u6216\u4e0b\u8f7d\u9875\uff09', '\u61c9\u7528\u7a0b\u5f0f\uff1aGPROXY Desktop\uff08Microsoft Store \u6216\u4e0b\u8f09\u9801\uff09')
    app_guide          = @('GPROXY Desktop is installed from Microsoft Store or the download page:', 'GPROXY Desktop \u8bf7\u4ece Microsoft Store \u6216\u4e0b\u8f7d\u9875\u5b89\u88c5\uff1a', 'GPROXY Desktop \u8acb\u5f9e Microsoft Store \u6216\u4e0b\u8f09\u9801\u5b89\u88dd\uff1a')
    app_open           = @('Open: 1) Microsoft Store  2) download page  3) nothing', '\u6253\u5f00\uff1a1) Microsoft Store  2) \u4e0b\u8f7d\u9875  3) \u4e0d\u6253\u5f00', '\u958b\u555f\uff1a1) Microsoft Store  2) \u4e0b\u8f09\u9801  3) \u4e0d\u958b\u555f')
    pick_mirror        = @('Download source', '\u4e0b\u8f7d\u6e90', '\u4e0b\u8f09\u4f86\u6e90')
    mirror_cnb         = @('CNB (fastest in mainland China)', 'CNB\uff08\u4e2d\u56fd\u5927\u9646\u63a8\u8350\uff09', 'CNB\uff08\u4e2d\u570b\u5927\u9678\u5efa\u8b70\uff09')
    existing           = @('Found an existing instance configuration: {0}', '\u53d1\u73b0\u5df2\u6709\u5b9e\u4f8b\u914d\u7f6e\uff1a{0}', '\u767c\u73fe\u65e2\u6709\u5be6\u4f8b\u8a2d\u5b9a\uff1a{0}')
    reconfigure        = @('Reconfigure it? The master key is kept. (y/N)', '\u91cd\u65b0\u914d\u7f6e\uff1f\u4e3b\u5bc6\u94a5\u4f1a\u4fdd\u7559\u3002(y/N)', '\u91cd\u65b0\u8a2d\u5b9a\uff1f\u4e3b\u91d1\u9470\u6703\u4fdd\u7559\u3002(y/N)')
    want_configure     = @('Set up an instance now (database, administrator, proxy)? (Y/n)', '\u73b0\u5728\u914d\u7f6e\u5b9e\u4f8b\uff08\u6570\u636e\u5e93\u3001\u7ba1\u7406\u5458\u3001\u4ee3\u7406\uff09\uff1f(Y/n)', '\u73fe\u5728\u8a2d\u5b9a\u5be6\u4f8b\uff08\u8cc7\u6599\u5eab\u3001\u7ba1\u7406\u54e1\u3001\u4ee3\u7406\uff09\uff1f(Y/n)')
    q_home             = @('Instance directory', '\u5b9e\u4f8b\u76ee\u5f55', '\u5be6\u4f8b\u76ee\u9304')
    q_db               = @('Database', '\u6570\u636e\u5e93', '\u8cc7\u6599\u5eab')
    db_sqlite          = @('SQLite file (recommended for one instance)', 'SQLite \u6587\u4ef6\uff08\u5355\u5b9e\u4f8b\u63a8\u8350\uff09', 'SQLite \u6a94\u6848\uff08\u55ae\u4e00\u5be6\u4f8b\u5efa\u8b70\uff09')
    db_dsn             = @('PostgreSQL / MySQL connection string (DSN)', 'PostgreSQL / MySQL \u8fde\u63a5\u4e32\uff08DSN\uff09', 'PostgreSQL / MySQL \u9023\u7dda\u5b57\u4e32\uff08DSN\uff09')
    q_db_path          = @('SQLite database file', 'SQLite \u6570\u636e\u5e93\u6587\u4ef6', 'SQLite \u8cc7\u6599\u5eab\u6a94\u6848')
    q_dsn              = @('DSN (postgres://user:pass@host:5432/db or mysql://...)', 'DSN\uff08postgres://user:pass@host:5432/db \u6216 mysql://...\uff09', 'DSN\uff08postgres://user:pass@host:5432/db \u6216 mysql://...\uff09')
    bad_dsn            = @('A DSN must start with postgres://, postgresql://, mysql:// or sqlite://.', 'DSN \u5fc5\u987b\u4ee5 postgres://\u3001postgresql://\u3001mysql:// \u6216 sqlite:// \u5f00\u5934\u3002', 'DSN \u5fc5\u9808\u4ee5 postgres://\u3001postgresql://\u3001mysql:// \u6216 sqlite:// \u958b\u982d\u3002')
    q_host             = @('Listen address (0.0.0.0 to accept other devices)', '\u76d1\u542c\u5730\u5740\uff08\u5141\u8bb8\u5176\u4ed6\u8bbe\u5907\u8bbf\u95ee\u8bf7\u586b 0.0.0.0\uff09', '\u76e3\u807d\u4f4d\u5740\uff08\u5141\u8a31\u5176\u4ed6\u88dd\u7f6e\u5b58\u53d6\u8acb\u586b 0.0.0.0\uff09')
    q_port             = @('Listen port', '\u76d1\u542c\u7aef\u53e3', '\u76e3\u807d\u57e0')
    bad_port           = @('The port must be a number from 1 to 65535.', '\u7aef\u53e3\u5fc5\u987b\u662f 1 \u5230 65535 \u4e4b\u95f4\u7684\u6570\u5b57\u3002', '\u57e0\u5fc5\u9808\u662f 1 \u5230 65535 \u4e4b\u9593\u7684\u6578\u5b57\u3002')
    q_admin            = @('Administrator username', '\u7ba1\u7406\u5458\u7528\u6237\u540d', '\u7ba1\u7406\u54e1\u4f7f\u7528\u8005\u540d\u7a31')
    q_password         = @('Administrator password (Enter to generate one; existing instance: Enter keeps it)', '\u7ba1\u7406\u5458\u5bc6\u7801\uff08\u56de\u8f66\u81ea\u52a8\u751f\u6210\uff1b\u5df2\u6709\u5b9e\u4f8b\u56de\u8f66\u5219\u4fdd\u6301\u4e0d\u53d8\uff09', '\u7ba1\u7406\u54e1\u5bc6\u78bc\uff08Enter \u81ea\u52d5\u7522\u751f\uff1b\u65e2\u6709\u5be6\u4f8b\u6309 Enter \u5247\u7dad\u6301\u4e0d\u8b8a\uff09')
    q_password2        = @('Repeat the password', '\u518d\u6b21\u8f93\u5165\u5bc6\u7801', '\u518d\u6b21\u8f38\u5165\u5bc6\u78bc')
    password_mismatch  = @('The passwords do not match.', '\u4e24\u6b21\u8f93\u5165\u7684\u5bc6\u7801\u4e0d\u4e00\u81f4\u3002', '\u5169\u6b21\u8f38\u5165\u7684\u5bc6\u78bc\u4e0d\u4e00\u81f4\u3002')
    password_short     = @('The password needs at least 8 characters.', '\u5bc6\u7801\u81f3\u5c11\u9700\u8981 8 \u4e2a\u5b57\u7b26\u3002', '\u5bc6\u78bc\u81f3\u5c11\u9700\u8981 8 \u500b\u5b57\u5143\u3002')
    q_proxy            = @('Global upstream proxy (e.g. http://127.0.0.1:7890, socks5h://...; Enter for none)', '\u5168\u5c40\u4e0a\u6e38\u4ee3\u7406\uff08\u5982 http://127.0.0.1:7890\u3001socks5h://...\uff1b\u56de\u8f66\u8868\u793a\u4e0d\u4f7f\u7528\uff09', '\u5168\u57df\u4e0a\u6e38\u4ee3\u7406\uff08\u5982 http://127.0.0.1:7890\u3001socks5h://...\uff1bEnter \u8868\u793a\u4e0d\u4f7f\u7528\uff09')
    bad_proxy          = @('Proxy URLs start with http://, https://, socks5:// or socks5h://.', '\u4ee3\u7406\u5730\u5740\u987b\u4ee5 http://\u3001https://\u3001socks5:// \u6216 socks5h:// \u5f00\u5934\u3002', '\u4ee3\u7406\u4f4d\u5740\u9808\u4ee5 http://\u3001https://\u3001socks5:// \u6216 socks5h:// \u958b\u982d\u3002')
    bad_quote          = @('Values cannot contain quotes or newlines.', '\u53d6\u503c\u4e0d\u80fd\u5305\u542b\u5f15\u53f7\u6216\u6362\u884c\u3002', '\u53d6\u503c\u4e0d\u80fd\u5305\u542b\u5f15\u865f\u6216\u63db\u884c\u3002')
    q_service          = @('Run GPROXY as a background task that starts at sign-in? (y/N)', '\u6ce8\u518c\u4e3a\u767b\u5f55\u65f6\u81ea\u52a8\u542f\u52a8\u7684\u540e\u53f0\u4efb\u52a1\uff1f(y/N)', '\u8a3b\u518a\u70ba\u767b\u5165\u6642\u81ea\u52d5\u555f\u52d5\u7684\u80cc\u666f\u5de5\u4f5c\uff1f(y/N)')
    summary            = @('About to install:', '\u5373\u5c06\u5b89\u88c5\uff1a', '\u5373\u5c07\u5b89\u88dd\uff1a')
    s_edition          = @('Edition', '\u7248\u672c', '\u7248\u672c')
    s_platform         = @('Platform', '\u5e73\u53f0', '\u5e73\u53f0')
    s_channel          = @('Channel', '\u901a\u9053', '\u901a\u9053')
    s_mirror           = @('Source', '\u4e0b\u8f7d\u6e90', '\u4e0b\u8f09\u4f86\u6e90')
    s_dir              = @('Command', '\u547d\u4ee4', '\u6307\u4ee4')
    s_home             = @('Instance', '\u5b9e\u4f8b', '\u5be6\u4f8b')
    s_db               = @('Database', '\u6570\u636e\u5e93', '\u8cc7\u6599\u5eab')
    s_listen           = @('Listen', '\u76d1\u542c', '\u76e3\u807d')
    s_admin            = @('Admin', '\u7ba1\u7406\u5458', '\u7ba1\u7406\u54e1')
    s_proxy            = @('Proxy', '\u4ee3\u7406', '\u4ee3\u7406')
    s_service          = @('Service', '\u670d\u52a1', '\u670d\u52d9')
    s_keep             = @('keep existing configuration', '\u4fdd\u7559\u73b0\u6709\u914d\u7f6e', '\u4fdd\u7559\u73fe\u6709\u8a2d\u5b9a')
    s_none             = @('none', '\u65e0', '\u7121')
    s_yes              = @('yes', '\u662f', '\u662f')
    s_no               = @('no', '\u5426', '\u5426')
    s_generated        = @('generated', '\u81ea\u52a8\u751f\u6210', '\u81ea\u52d5\u7522\u751f')
    s_set              = @('set', '\u5df2\u8bbe\u7f6e', '\u5df2\u8a2d\u5b9a')
    proceed            = @('Continue? (Y/n)', '\u7ee7\u7eed\uff1f(Y/n)', '\u7e7c\u7e8c\uff1f(Y/n)')
    aborted            = @('Cancelled.', '\u5df2\u53d6\u6d88\u3002', '\u5df2\u53d6\u6d88\u3002')
    resolving          = @('Looking up the latest {0} version...', '\u6b63\u5728\u67e5\u8be2 {0} \u901a\u9053\u7684\u6700\u65b0\u7248\u672c\u2026\u2026', '\u6b63\u5728\u67e5\u8a62 {0} \u901a\u9053\u7684\u6700\u65b0\u7248\u672c\u2026\u2026')
    downloading        = @('Downloading {0}', '\u6b63\u5728\u4e0b\u8f7d {0}', '\u6b63\u5728\u4e0b\u8f09 {0}')
    verified           = @('SHA-256 verified.', 'SHA-256 \u6821\u9a8c\u901a\u8fc7\u3002', 'SHA-256 \u9a57\u8b49\u901a\u904e\u3002')
    bad_sum            = @('Checksum mismatch for {0}; nothing was installed.', '{0} \u6821\u9a8c\u548c\u4e0d\u5339\u914d\uff0c\u672a\u5b89\u88c5\u4efb\u4f55\u6587\u4ef6\u3002', '{0} \u6821\u9a57\u548c\u4e0d\u7b26\uff0c\u672a\u5b89\u88dd\u4efb\u4f55\u6a94\u6848\u3002')
    no_sum             = @('No SHA-256 entry for {0} in SHA256SUMS.', 'SHA256SUMS \u4e2d\u6ca1\u6709 {0} \u7684\u6761\u76ee\u3002', 'SHA256SUMS \u4e2d\u6c92\u6709 {0} \u7684\u689d\u76ee\u3002')
    installed          = @('Installed GPROXY {0}: {1}', '\u5df2\u5b89\u88c5 GPROXY {0}\uff1a{1}', '\u5df2\u5b89\u88dd GPROXY {0}\uff1a{1}')
    in_use             = @('{0} is running; restart it to use the new version.', '{0} \u6b63\u5728\u8fd0\u884c\uff0c\u91cd\u542f\u540e\u624d\u4f1a\u4f7f\u7528\u65b0\u7248\u672c\u3002', '{0} \u6b63\u5728\u57f7\u884c\uff0c\u91cd\u65b0\u555f\u52d5\u5f8c\u624d\u6703\u4f7f\u7528\u65b0\u7248\u672c\u3002')
    path_added         = @('Added {0} to your user PATH; open a new terminal to use it.', '\u5df2\u5c06 {0} \u52a0\u5165\u7528\u6237 PATH\uff0c\u8bf7\u6253\u5f00\u65b0\u7684\u7ec8\u7aef\u4f7f\u7528\u3002', '\u5df2\u5c07 {0} \u52a0\u5165\u4f7f\u7528\u8005 PATH\uff0c\u8acb\u958b\u555f\u65b0\u7684\u7d42\u7aef\u6a5f\u4f7f\u7528\u3002')
    writing_env        = @('Writing {0}', '\u6b63\u5728\u5199\u5165 {0}', '\u6b63\u5728\u5beb\u5165 {0}')
    bootstrapping      = @('Preparing the database and administrator...', '\u6b63\u5728\u521d\u59cb\u5316\u6570\u636e\u5e93\u4e0e\u7ba1\u7406\u5458\u2026\u2026', '\u6b63\u5728\u521d\u59cb\u5316\u8cc7\u6599\u5eab\u8207\u7ba1\u7406\u54e1\u2026\u2026')
    setting_proxy      = @('Setting the global proxy...', '\u6b63\u5728\u8bbe\u7f6e\u5168\u5c40\u4ee3\u7406\u2026\u2026', '\u6b63\u5728\u8a2d\u5b9a\u5168\u57df\u4ee3\u7406\u2026\u2026')
    proxy_failed       = @('Could not set the proxy; set it later in Console > Settings.', '\u4ee3\u7406\u8bbe\u7f6e\u5931\u8d25\uff0c\u8bf7\u7a0d\u540e\u5728 \u63a7\u5236\u53f0 > \u8bbe\u7f6e \u4e2d\u914d\u7f6e\u3002', '\u4ee3\u7406\u8a2d\u5b9a\u5931\u6557\uff0c\u8acb\u7a0d\u5f8c\u5728 \u63a7\u5236\u53f0 > \u8a2d\u5b9a \u4e2d\u8a2d\u5b9a\u3002')
    installing_service = @('Registering the background task...', '\u6b63\u5728\u6ce8\u518c\u540e\u53f0\u4efb\u52a1\u2026\u2026', '\u6b63\u5728\u8a3b\u518a\u80cc\u666f\u5de5\u4f5c\u2026\u2026')
    service_failed     = @('Task registration failed; start GPROXY manually with the command below.', '\u4efb\u52a1\u6ce8\u518c\u5931\u8d25\uff0c\u8bf7\u7528\u4e0b\u9762\u7684\u547d\u4ee4\u624b\u52a8\u542f\u52a8\u3002', '\u5de5\u4f5c\u8a3b\u518a\u5931\u6557\uff0c\u8acb\u4ee5\u4e0b\u65b9\u6307\u4ee4\u624b\u52d5\u555f\u52d5\u3002')
    restarting         = @('Restarting the scheduled task...', '\u6b63\u5728\u91cd\u542f\u8ba1\u5212\u4efb\u52a1\u2026\u2026', '\u6b63\u5728\u91cd\u65b0\u555f\u52d5\u6392\u7a0b\u5de5\u4f5c\u2026\u2026')
    next               = @('Start the server:', '\u542f\u52a8\u670d\u52a1\uff1a', '\u555f\u52d5\u670d\u52d9\uff1a')
    running            = @('GPROXY is running as the scheduled task "gproxy".', 'GPROXY \u5df2\u4f5c\u4e3a\u8ba1\u5212\u4efb\u52a1\u201cgproxy\u201d\u8fd0\u884c\u3002', 'GPROXY \u5df2\u4f5c\u70ba\u6392\u7a0b\u5de5\u4f5c\u300cgproxy\u300d\u57f7\u884c\u3002')
    console_at         = @('Console: {0}', '\u63a7\u5236\u53f0\uff1a{0}', '\u63a7\u5236\u53f0\uff1a{0}')
    save_creds         = @('Save the administrator credentials printed above; they are shown only once.', '\u8bf7\u4fdd\u5b58\u4e0a\u9762\u6253\u5370\u7684\u7ba1\u7406\u5458\u51ed\u636e\uff0c\u5b83\u4eec\u53ea\u663e\u793a\u4e00\u6b21\u3002', '\u8acb\u4fdd\u5b58\u4e0a\u65b9\u5217\u5370\u7684\u7ba1\u7406\u54e1\u6191\u8b49\uff0c\u5b83\u5011\u53ea\u6703\u986f\u793a\u4e00\u6b21\u3002')
    keep_env           = @('Back up {0}: it holds the master key that decrypts stored credentials.', '\u8bf7\u5907\u4efd {0}\uff1a\u5176\u4e2d\u7684\u4e3b\u5bc6\u94a5\u7528\u4e8e\u89e3\u5bc6\u5df2\u5b58\u50a8\u7684\u51ed\u8bc1\u3002', '\u8acb\u5099\u4efd {0}\uff1a\u5176\u4e2d\u7684\u4e3b\u91d1\u9470\u7528\u65bc\u89e3\u5bc6\u5df2\u5132\u5b58\u7684\u6191\u8b49\u3002')
    next_plain         = @('First start prints a generated administrator password and API key once; save them.', '\u9996\u6b21\u542f\u52a8\u4f1a\u6253\u5370\u4e00\u6b21\u81ea\u52a8\u751f\u6210\u7684\u7ba1\u7406\u5458\u5bc6\u7801\u4e0e API \u5bc6\u94a5\uff0c\u8bf7\u59a5\u5584\u4fdd\u5b58\u3002', '\u9996\u6b21\u555f\u52d5\u6703\u5217\u5370\u4e00\u6b21\u81ea\u52d5\u7522\u751f\u7684\u7ba1\u7406\u54e1\u5bc6\u78bc\u8207 API \u91d1\u9470\uff0c\u8acb\u59a5\u5584\u4fdd\u5b58\u3002')
    docs               = @('Docs', '\u6587\u6863', '\u6587\u4ef6')
    app_hint           = @('Prefer the desktop app? Get GPROXY Desktop from Microsoft Store or', '\u60f3\u7528\u684c\u9762 App\uff1f\u8bf7\u4ece Microsoft Store \u6216\u6b64\u5904\u83b7\u53d6 GPROXY Desktop\uff1a', '\u60f3\u7528\u684c\u9762 App\uff1f\u8acb\u5f9e Microsoft Store \u6216\u6b64\u8655\u53d6\u5f97 GPROXY Desktop\uff1a')
}

function Normalize-Lang([string]$value) {
    switch -Regex ($value) {
        '^(1|en.*)$' { return 'en' }
        '^(2|zh-cn|zh_cn|zh-hans.*|zh-sg|zh|cn)$' { return 'zh-CN' }
        '^(3|zh-tw|zh_tw|zh-hk|zh-mo|zh-hant.*|tw)$' { return 'zh-TW' }
        default { return '' }
    }
}

function T([string]$key) {
    $index = switch ($script:Lang) { 'zh-CN' { 1 } 'zh-TW' { 2 } default { 0 } }
    return [regex]::Unescape($Messages[$key][$index])
}

function Say([string]$key) { Write-Host ((T $key) -f $args) }
function Warn([string]$key) { Write-Host ((T $key) -f $args) -ForegroundColor Yellow }
function Fail([string]$key) { throw ((T $key) -f $args) }

$Interactive = (-not $Yes) -and [Environment]::UserInteractive
try { if ([Console]::IsInputRedirected) { $Interactive = $false } } catch { }

function Ask([string]$prompt, [string]$default = '') {
    $label = if ($default) { "$prompt [$default]" } else { $prompt }
    $answer = Read-Host $label
    if ([string]::IsNullOrEmpty($answer)) { return $default }
    return $answer.Trim()
}

function Ask-Secret([string]$prompt) {
    $secure = Read-Host $prompt -AsSecureString
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try { return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
}

function Is-No([string]$answer) { return $answer -match '^(n|no)$' -or $answer -eq [regex]::Unescape('\u5426') }
function Is-Yes([string]$answer) { return $answer -match '^(y|yes)$' -or $answer -eq [regex]::Unescape('\u662f') }

if ($Lang) { $Lang = Normalize-Lang $Lang }
if (-not $Lang) {
    $Lang = Normalize-Lang (Get-UICulture).Name.ToLowerInvariant()
    if (-not $Lang) { $Lang = 'en' }
    if ($Interactive) {
        $default = switch ($Lang) { 'zh-CN' { '2' } 'zh-TW' { '3' } default { '1' } }
        Write-Host ([regex]::Unescape('Language / \u8bed\u8a00 / \u8a9e\u8a00'))
        Write-Host '  1) English'
        Write-Host ([regex]::Unescape('  2) \u7b80\u4f53\u4e2d\u6587'))
        Write-Host ([regex]::Unescape('  3) \u7e41\u9ad4\u4e2d\u6587'))
        $picked = Normalize-Lang (Ask '>' $default)
        if ($picked) { $Lang = $picked }
    }
}

$DocsBase = switch ($Lang) { 'zh-CN' { "$Site/zh-cn" } 'zh-TW' { "$Site/zh-tw" } default { $Site } }
$DownloadPage = "$DocsBase/download/"

# ---------------------------------------------------------------- platform

function Detect-Arch {
    $arch = ''
    try { $arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString() } catch { }
    if (-not $arch) {
        $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    }
    switch -Regex ($arch) {
        '^(X64|AMD64)$' { return 'windows-x86_64' }
        '^(Arm64|ARM64)$' { return 'windows-aarch64' }
        default { return '' }
    }
}

$AllTargets = @('windows-x86_64', 'windows-aarch64')
$Detected = Detect-Arch
if (-not $Target) {
    $Target = $Detected
    if ($Interactive) {
        $keep = 'n'
        if ($Target) {
            Say detected $Target
            $keep = Ask (T 'keep_target')
        }
        if ((Is-No $keep) -or -not $Target) {
            Write-Host (T 'pick_target')
            for ($i = 0; $i -lt $AllTargets.Count; $i++) { Write-Host ('  {0}) {1}' -f ($i + 1), $AllTargets[$i]) }
            $picked = Ask (T 'choice') '1'
            if ($picked -match '^\d+$' -and [int]$picked -ge 1 -and [int]$picked -le $AllTargets.Count) {
                $Target = $AllTargets[[int]$picked - 1]
            } elseif ($AllTargets -contains $picked) {
                $Target = $picked
            }
        }
    }
}
if (-not $Target) { $Target = 'windows-x86_64' }
if ($AllTargets -notcontains $Target) { throw "unsupported target: $Target" }
if ($Detected -and $Detected -ne $Target) { Warn foreign_target $Target }

# ---------------------------------------------------------------- edition

if (-not $Edition) {
    $Edition = 'cli'
    if ($Interactive) {
        Write-Host (T 'pick_edition')
        Write-Host ('  1) ' + (T 'ed_cli'))
        Write-Host ('  2) ' + (T 'ed_headless'))
        Write-Host ('  3) ' + (T 'ed_app'))
        switch (Ask (T 'choice') '1') { { $_ -in '2', 'headless' } { $Edition = 'headless' } { $_ -in '3', 'app' } { $Edition = 'app' } }
    }
}

if ($Edition -in 'app', 'application', 'desktop') {
    Say app_guide
    Write-Host "  Microsoft Store: https://apps.microsoft.com/detail/$($StoreDesktop)?mode=direct"
    Write-Host "  winget install --id $StoreDesktop --source msstore"
    Write-Host "  $DownloadPage"
    if ($Interactive) {
        switch (Ask (T 'app_open') '1') {
            '1' { Start-Process "ms-windows-store://pdp/?productid=$StoreDesktop" }
            '2' { Start-Process $DownloadPage }
        }
    }
    return
}
if ($Edition -notin 'cli', 'headless') { throw "unknown edition: $Edition" }

# ---------------------------------------------------------------- source

$Channel = switch ($Channel) {
    { $_ -in 'release', 'stable' } { 'release' }
    { $_ -in 'beta', 'staging' } { 'beta' }
    { $_ -in 'dev', 'nightly' } { 'dev' }
    default { throw "unknown channel: $Channel" }
}

if (-not $Mirror) {
    $Mirror = if ($Lang -eq 'zh-CN') { 'cnb' } else { 'github' }
    if ($Interactive) {
        $default = if ($Mirror -eq 'cnb') { '3' } else { '1' }
        Write-Host (T 'pick_mirror')
        Write-Host '  1) GitHub'
        Write-Host '  2) GitLab'
        Write-Host ('  3) ' + (T 'mirror_cnb'))
        switch (Ask (T 'choice') $default) {
            { $_ -in '1', 'github' } { $Mirror = 'github' }
            { $_ -in '2', 'gitlab' } { $Mirror = 'gitlab' }
            { $_ -in '3', 'cnb' } { $Mirror = 'cnb' }
        }
    }
}

function Asset-Url([string]$tag, [string]$name) {
    switch ($Mirror) {
        'github' { return "https://github.com/LeenHawk/gproxy/releases/download/$tag/$name" }
        'gitlab' { return "https://gitlab.com/leenhawk1/gproxy/-/releases/$tag/downloads/$name" }
        'cnb' { return "https://cnb.cool/LeenHawk/gproxy/-/releases/download/$tag/$name" }
    }
}
if ($Mirror -notin 'github', 'gitlab', 'cnb') { throw "unknown mirror: $Mirror" }

# ---------------------------------------------------------------- instance

if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\gproxy' }
if (-not $DataDir) { $DataDir = Join-Path $env:LOCALAPPDATA 'gproxy' }
$EnvFile = Join-Path $DataDir '.env'

function Env-Value([string]$name) {
    if (-not (Test-Path -LiteralPath $EnvFile)) { return '' }
    $value = ''
    foreach ($line in Get-Content -LiteralPath $EnvFile) {
        if ($line -match "^\s*(export\s+)?$name=(.*)$") { $value = $Matches[2].Trim() }
    }
    if ($value -match "^'(.*)'$" -or $value -match '^"(.*)"$') { $value = $Matches[1] }
    return $value
}

# Reconfiguring starts from what the existing .env says, so changing one
# setting does not reset the others.
function Load-Current {
    $script:CurHost = Env-Value 'GPROXY_HOST'
    $script:CurPort = Env-Value 'GPROXY_PORT'
    $script:CurDsn = ''
    $script:CurDb = ''
    $current = Env-Value 'GPROXY_DSN'
    if ($current -match '^(postgres|postgresql|mysql)://') { $script:CurDsn = $current }
    elseif ($current) { $script:CurDb = $current }
    if (-not $script:CurDb) { $script:CurDb = Join-Path $DataDir 'gproxy.db' }
}
Load-Current

$Configure = $null
if ($NoConfigure) { $Configure = $false }
elseif ($DbPath -or $Dsn -or $ListenHost -or $Port -or $AdminUser -or $AdminPassword -or $Proxy -or $Service -or $NoService) { $Configure = $true }

$Reconfigure = $true
if ($null -eq $Configure) {
    if (Test-Path -LiteralPath $EnvFile) {
        $Reconfigure = $false
        if ($Interactive) {
            Say existing $EnvFile
            if (Is-Yes (Ask (T 'reconfigure'))) { $Reconfigure = $true }
        }
        $Configure = $true
    } elseif ($Interactive) {
        $Configure = -not (Is-No (Ask (T 'want_configure')))
    } else {
        $Configure = $true
    }
}

function Valid-Value([string]$value) { return $value -notmatch "['`"\r\n]" }
function Valid-Port([string]$value) { return $value -match '^\d{1,5}$' -and [int]$value -ge 1 -and [int]$value -le 65535 }
function Valid-Dsn([string]$value) { return $value -match '^(postgres|postgresql|mysql|sqlite)://' }
function Valid-Proxy([string]$value) { return $value -match '^(http|https|socks5|socks5h)://.+' -and (Valid-Value $value) }

$ServiceChoice = if ($Service) { $true } elseif ($NoService) { $false } else { $null }

if ($Configure -and $Reconfigure -and $Interactive) {
    do {
        $DataDir = Ask (T 'q_home') $DataDir
        $ok = Valid-Value $DataDir
        if (-not $ok) { Warn bad_quote }
    } until ($ok)
    $EnvFile = Join-Path $DataDir '.env'
    Load-Current
    if (-not $DbPath -and -not $Dsn) {
        Write-Host (T 'q_db')
        Write-Host ('  1) ' + (T 'db_sqlite'))
        Write-Host ('  2) ' + (T 'db_dsn'))
        $default = if ($CurDsn) { '2' } else { '1' }
        if ((Ask (T 'choice') $default) -eq '2') {
            do {
                $Dsn = Ask (T 'q_dsn') $CurDsn
                $ok = (Valid-Dsn $Dsn) -and (Valid-Value $Dsn)
                if (-not $ok) { Warn bad_dsn }
            } until ($ok)
        } else {
            do {
                $DbPath = Ask (T 'q_db_path') $CurDb
                $ok = Valid-Value $DbPath
                if (-not $ok) { Warn bad_quote }
            } until ($ok)
        }
    }
    if (-not $ListenHost) { $ListenHost = Ask (T 'q_host') $(if ($CurHost) { $CurHost } else { '127.0.0.1' }) }
    while (-not (Valid-Port $Port)) {
        $Port = Ask (T 'q_port') $(if ($CurPort) { $CurPort } else { '8787' })
        if (-not (Valid-Port $Port)) { Warn bad_port }
    }
    if (-not $AdminUser) { $AdminUser = Ask (T 'q_admin') 'admin' }
    while (-not $AdminPassword) {
        $first = Ask-Secret (T 'q_password')
        if (-not $first) { break }
        if ($first.Length -lt 8) { Warn password_short; continue }
        $second = Ask-Secret (T 'q_password2')
        if ($first -ceq $second) { $AdminPassword = $first } else { Warn password_mismatch }
    }
    while (-not $Proxy) {
        $Proxy = Ask (T 'q_proxy')
        if (-not $Proxy) { break }
        if (-not (Valid-Proxy $Proxy)) { Warn bad_proxy; $Proxy = '' }
    }
    if ($null -eq $ServiceChoice) { $ServiceChoice = Is-Yes (Ask (T 'q_service')) }
}

if (-not $ListenHost) { $ListenHost = if ($CurHost) { $CurHost } else { '127.0.0.1' } }
if (-not $Port) { $Port = if ($CurPort) { $CurPort } else { '8787' } }
if (-not $DbPath -and -not $Dsn) { if ($CurDsn) { $Dsn = $CurDsn } else { $DbPath = $CurDb } }
if (-not $AdminUser) { $AdminUser = 'admin' }
if ($null -eq $ServiceChoice) { $ServiceChoice = $false }
if (-not (Valid-Port $Port)) { Fail bad_port }
if ($Dsn -and -not (Valid-Dsn $Dsn)) { Fail bad_dsn }
if ($Proxy -and -not (Valid-Proxy $Proxy)) { Fail bad_proxy }
if ($AdminPassword -and $AdminPassword.Length -lt 8) { Fail password_short }
foreach ($value in @($DataDir, $DbPath, $Dsn, $ListenHost, $AdminUser)) { if (-not (Valid-Value $value)) { Fail bad_quote } }

# ---------------------------------------------------------------- summary

$Prefix = if ($Edition -eq 'headless') { 'gproxy-headless' } else { 'gproxy' }
$Asset = "$Prefix-$Target.zip"
$Exe = Join-Path $InstallDir 'gproxy.exe'

function Row([string]$key, [string]$value) { Write-Host ('  - {0}: {1}' -f (T $key), $value) }
function Yes-No([bool]$value) { if ($value) { T 's_yes' } else { T 's_no' } }

Say summary
Row s_edition $Edition
Row s_platform $Target
Row s_channel $Channel
Row s_mirror $Mirror
Row s_dir $Exe
if ($Configure) {
    Row s_home $DataDir
    if ($Reconfigure) {
        if ($Dsn) { Row s_db (($Dsn -split '://')[0] + '://...') } else { Row s_db $DbPath }
        Row s_listen "${ListenHost}:$Port"
        $how = if ($AdminPassword) { T 's_set' } else { T 's_generated' }
        Row s_admin "$AdminUser ($how)"
        Row s_proxy $(if ($Proxy) { $Proxy } else { T 's_none' })
        Row s_service (Yes-No $ServiceChoice)
    } else {
        Row s_db (T 's_keep')
    }
}
if ($Interactive -and (Is-No (Ask (T 'proceed')))) { Say aborted; return }

# ---------------------------------------------------------------- download

$Work = Join-Path ([IO.Path]::GetTempPath()) ('gproxy-install-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Work | Out-Null

# Native commands run with 'Continue': under 'Stop', Windows PowerShell 5.1
# turns any redirected stderr line into a terminating error.
function Invoke-Quiet([string]$exe) {
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $exe @args *> $null; return $LASTEXITCODE -eq 0 }
    catch { return $false }
    finally { $ErrorActionPreference = $saved }
}

function Fetch([string]$url, [string]$path) {
    Invoke-WebRequest -Uri $url -OutFile $path -UseBasicParsing
}

$Utf8 = New-Object Text.UTF8Encoding $false

# Run gproxy in the instance directory, where it reads .env.
function Invoke-Gproxy {
    $saved = $env:GPROXY_LOG_FILTER
    $savedPreference = $ErrorActionPreference
    $env:GPROXY_LOG_FILTER = 'warn'
    $ErrorActionPreference = 'Continue'
    Push-Location -LiteralPath $DataDir
    try { & $Exe @args | Out-Host; return $LASTEXITCODE -eq 0 }
    finally { Pop-Location; $env:GPROXY_LOG_FILTER = $saved; $ErrorActionPreference = $savedPreference }
}

try {
    # Resolve the channel to a concrete tag through its manifest, so the
    # package and SHA256SUMS always come from the same release.
    Say resolving $Channel
    $manifestUrl = switch ($Channel) {
        'release' { if ($Mirror -eq 'github') { 'https://github.com/LeenHawk/gproxy/releases/latest/download/manifest.json' } else { Asset-Url 'release' 'manifest.json' } }
        'beta' { Asset-Url 'staging' 'manifest.json' }
        'dev' { Asset-Url 'nightly' 'manifest.json' }
    }
    Fetch $manifestUrl (Join-Path $Work 'manifest.json')
    $Version = (Get-Content -Raw -LiteralPath (Join-Path $Work 'manifest.json') | ConvertFrom-Json).version
    if (-not $Version) { throw "manifest.json has no version: $manifestUrl" }
    $Tag = switch ($Channel) { 'release' { "v$Version" } 'beta' { 'staging' } 'dev' { 'nightly' } }

    Fetch (Asset-Url $Tag 'SHA256SUMS') (Join-Path $Work 'SHA256SUMS')
    Say downloading "$Asset ($Version)"
    $zip = Join-Path $Work $Asset
    Fetch (Asset-Url $Tag $Asset) $zip

    $expected = ''
    foreach ($line in Get-Content -LiteralPath (Join-Path $Work 'SHA256SUMS')) {
        $parts = $line -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $Asset) { $expected = $parts[0]; break }
    }
    if (-not $expected) { Fail no_sum $Asset }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash -ne $expected.ToUpperInvariant()) { Fail bad_sum $Asset }
    Say verified

    # ------------------------------------------------------------ install

    Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $Work 'out') -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $running = @(Get-Process -Name gproxy -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe })
    # A running executable cannot be overwritten but can be renamed.
    $old = "$Exe.old"
    if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue }
    if (Test-Path -LiteralPath $Exe) { Move-Item -LiteralPath $Exe -Destination $old -Force }
    Copy-Item -LiteralPath (Join-Path $Work 'out\gproxy.exe') -Destination $Exe
    if (-not $running) { Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue }
    Write-Host ''
    Say installed $Version $Exe

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (($userPath -split ';') -contains $InstallDir)) {
        $joined = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable('Path', $joined, 'User')
        $env:Path = "$env:Path;$InstallDir"
        Say path_added $InstallDir
    }

    # ------------------------------------------------------------ configure

    $taskExists = Invoke-Quiet schtasks.exe /query /tn gproxy

    function Restart-Task {
        if (-not $taskExists) { return }
        Say restarting
        Invoke-Quiet schtasks.exe /end /tn gproxy | Out-Null
        Get-Process -Name gproxy -ErrorAction SilentlyContinue | Where-Object { $_.Path -in $Exe, $old } | Stop-Process -Force
        Invoke-Quiet schtasks.exe /run /tn gproxy | Out-Null
    }

    $serviceRunning = $false
    if ($Configure -and $Reconfigure) {
        New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
        $masterKey = Env-Value 'GPROXY_MASTER_KEY'
        if (-not $masterKey) {
            $bytes = New-Object byte[] 32
            [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
            $masterKey = -join ($bytes | ForEach-Object { $_.ToString('x2') })
        }
        $dbValue = if ($Dsn) { $Dsn } else { $DbPath }
        Say writing_env $EnvFile
        $lines = @(
            '# Written by the GPROXY installer. Read by `gproxy` started in this directory',
            '# and by the scheduled task. Keep GPROXY_MASTER_KEY: it decrypts stored credentials.',
            "GPROXY_HOST='$ListenHost'",
            "GPROXY_PORT='$Port'",
            "GPROXY_DATA_DIR='$DataDir'",
            "GPROXY_DSN='$dbValue'",
            "GPROXY_MASTER_KEY='$masterKey'",
            "GPROXY_UPDATE_CHANNEL='$Channel'",
            "GPROXY_UPDATE_SOURCE='$Mirror'"
        )
        # No BOM: dotenv would read it as part of the first key.
        [IO.File]::WriteAllText($EnvFile, (($lines -join "`n") + "`n"), $Utf8)
        $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        Invoke-Quiet icacls.exe $EnvFile /inheritance:r /grant:r "*$($sid):F" | Out-Null
        if ($DbPath) { New-Item -ItemType Directory -Force -Path (Split-Path -Parent $DbPath) | Out-Null }

        Say bootstrapping
        $env:GPROXY_ADMIN_USER = $AdminUser
        if ($AdminPassword) { $env:GPROXY_ADMIN_PASSWORD = $AdminPassword }
        try {
            if (-not (Invoke-Gproxy bootstrap admin)) { throw 'gproxy bootstrap admin failed' }
        } finally {
            Remove-Item Env:GPROXY_ADMIN_USER, Env:GPROXY_ADMIN_PASSWORD -ErrorAction SilentlyContinue
        }

        if ($Proxy) {
            Say setting_proxy
            # Import only the settings row, so providers and credentials are
            # left exactly as they are.
            $ok = $false
            try {
                $export = Join-Path $Work 'export.json'
                if (Invoke-Gproxy export --out $export) {
                    $document = [IO.File]::ReadAllText($export, $Utf8) | ConvertFrom-Json
                    $settings = $document.data.settings
                    $settings.instance.proxy = [pscustomobject]@{ mode = 'explicit'; url = $Proxy }
                    $patch = [pscustomobject]@{
                        formatVersion  = $document.formatVersion
                        exportedAtMs   = $document.exportedAtMs
                        secretsOmitted = $true
                        secrets        = @()
                        data           = [pscustomobject]@{ settings = $settings }
                    }
                    $patchFile = Join-Path $Work 'settings.json'
                    [IO.File]::WriteAllText($patchFile, ($patch | ConvertTo-Json -Depth 32), $Utf8)
                    $ok = Invoke-Gproxy import --in $patchFile
                }
            } catch { $ok = $false }
            if (-not $ok) { Warn proxy_failed }
        }

        Restart-Task
        if ($ServiceChoice) {
            Say installing_service
            if (Invoke-Gproxy service install) { $serviceRunning = $true } else { Warn service_failed }
        }
    } elseif ($Configure) {
        Restart-Task
    }
    if ($running -and -not $taskExists) { Warn in_use $Exe }

    # ------------------------------------------------------------ next steps

    Write-Host ''
    if ($Configure) {
        if ($serviceRunning -or ($taskExists -and -not $Reconfigure)) {
            Say running
        } else {
            Say next
            Write-Host "  cd `"$DataDir`"; gproxy serve"
        }
        if ($Reconfigure) {
            if ($Edition -eq 'cli') {
                $consoleHost = if ($ListenHost -in '0.0.0.0', '::', '[::]') { '127.0.0.1' } else { $ListenHost }
                Say console_at "http://${consoleHost}:$Port/console/"
            }
            if (-not $AdminPassword) { Say save_creds }
            Say keep_env $EnvFile
        }
    } else {
        Say next
        Write-Host "  gproxy serve --data-dir `"$DataDir`""
        Say next_plain
    }
    Write-Host ('{0}: {1}/getting-started/quick-start/' -f (T 'docs'), $DocsBase)
    Write-Host ('{0} {1}' -f (T 'app_hint'), $DownloadPage)
} finally {
    Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
}
