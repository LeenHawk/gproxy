//! The terminal wizard's words, in the four languages the console ships.
//!
//! A table rather than a translation framework: the wizard says perhaps fifty
//! things, once per installation, and every other line this binary prints is
//! English on purpose. The texts follow the desktop wizard's (`setup.*` in
//! the console's locale files), so the two read as one product.

/// One of the console's languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Ru,
    ZhCn,
    ZhTw,
}

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Ru, Lang::ZhCn, Lang::ZhTw];

    /// The language's own name, which is how a picker should list it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
            Lang::ZhCn => "简体中文",
            Lang::ZhTw => "繁體中文",
        }
    }

    /// The POSIX locale (`zh_TW.UTF-8`, `ru_RU`, …) mapped onto the four, the
    /// way the console maps a browser's language.
    pub fn from_locale(locale: &str) -> Lang {
        let code = locale.to_ascii_lowercase().replace('_', "-");
        if code == "ru" || code.starts_with("ru-") {
            Lang::Ru
        } else if code == "zh" || code.starts_with("zh-") {
            let region = code.split(['.', '@']).next().unwrap_or_default();
            if ["zh-tw", "zh-hk", "zh-mo", "zh-hant"]
                .iter()
                .any(|tag| region.starts_with(tag))
            {
                Lang::ZhTw
            } else {
                Lang::ZhCn
            }
        } else {
            Lang::En
        }
    }

    /// The first of `LC_ALL`, `LC_MESSAGES` and `LANG` that is set, as POSIX
    /// orders them.
    pub fn detect() -> Lang {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
            .map_or(Lang::En, |value| Lang::from_locale(&value))
    }
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

macro_rules! messages {
    ($($key:ident => [$en:expr, $ru:expr, $zh_cn:expr, $zh_tw:expr $(,)?],)*) => {
        /// Everything the wizard says. `{}` marks where a value goes.
        #[derive(Clone, Copy, Debug)]
        pub enum Msg { $($key,)* }

        impl Lang {
            pub fn t(self, message: Msg) -> &'static str {
                match message {
                    $(Msg::$key => match self {
                        Lang::En => $en,
                        Lang::Ru => $ru,
                        Lang::ZhCn => $zh_cn,
                        Lang::ZhTw => $zh_tw,
                    },)*
                }
            }

            /// [`Lang::t`] with its one `{}` filled in.
            pub fn f(self, message: Msg, value: impl std::fmt::Display) -> String {
                self.t(message).replacen("{}", &value.to_string(), 1)
            }
        }
    };
}

messages! {
    Language => ["Language", "Язык", "语言", "語言"],
    Welcome => [
        "Welcome to GPROXY. This first run sets up the instance; your answers are saved to {}.",
        "Добро пожаловать в GPROXY. При первом запуске настраивается экземпляр; ответы будут сохранены в {}.",
        "欢迎使用 GPROXY。首次运行会完成实例设置，你的选择将保存到 {}。",
        "歡迎使用 GPROXY。首次執行會完成實例設定，你的選擇將儲存到 {}。",
    ],
    Connection => ["Instance settings", "Настройки экземпляра", "实例设置", "實例設定"],
    Host => ["Listening IP address", "IP-адрес для прослушивания", "监听 IP 地址", "監聽 IP 位址"],
    HostHelp => [
        "127.0.0.1 is this machine only; 0.0.0.0 accepts connections from the network",
        "127.0.0.1 — только этот компьютер; 0.0.0.0 — подключения из сети",
        "127.0.0.1 仅本机可访问；0.0.0.0 允许来自网络的连接",
        "127.0.0.1 僅本機可存取；0.0.0.0 允許來自網路的連線",
    ],
    HostInvalid => [
        "Enter a valid listening IP address.",
        "Введите корректный IP-адрес для прослушивания.",
        "请输入有效的监听 IP 地址。",
        "請輸入有效的監聽 IP 位址。",
    ],
    Port => ["Port", "Порт", "端口", "連接埠"],
    PortInvalid => [
        "Enter a port between 1 and 65535.",
        "Введите порт от 1 до 65535.",
        "请输入 1 到 65535 之间的端口。",
        "請輸入 1 到 65535 之間的連接埠。",
    ],
    DataDir => ["Data directory", "Каталог данных", "数据目录", "資料目錄"],
    DirectoryRequired => [
        "Enter a data directory.",
        "Укажите каталог данных.",
        "请输入数据目录。",
        "請輸入資料目錄。",
    ],
    Database => ["Database", "База данных", "数据库", "資料庫"],
    DatabaseFile => ["Database file", "Файл базы данных", "数据库文件", "資料庫檔案"],
    DatabaseUrl => ["Database connection URL", "URL подключения к базе данных", "数据库连接 URL", "資料庫連線 URL"],
    DatabaseInvalid => [
        "Enter a connection URL matching the selected database.",
        "Введите URL подключения, соответствующий выбранной базе данных.",
        "请输入与所选数据库类型一致的连接 URL。",
        "請輸入與所選資料庫類型一致的連線 URL。",
    ],
    Account => ["Administrator account", "Учётная запись администратора", "管理员账户", "管理員帳戶"],
    AdminUser => ["Administrator name", "Имя администратора", "管理员用户名", "管理員使用者名稱"],
    UserRequired => [
        "Enter an administrator name.",
        "Введите имя администратора.",
        "请输入管理员用户名。",
        "請輸入管理員使用者名稱。",
    ],
    Password => ["Administrator password", "Пароль администратора", "管理员密码", "管理員密碼"],
    Confirmation => ["Confirm password", "Подтвердите пароль", "确认密码", "確認密碼"],
    PasswordInvalid => [
        "Enter a password of at least 8 characters that is not only spaces.",
        "Введите пароль не короче 8 символов, состоящий не только из пробелов.",
        "请输入至少 8 个字符的非空白密码。",
        "請輸入至少 8 個字元的非空白密碼。",
    ],
    PasswordMismatch => [
        "The passwords do not match.",
        "Пароли не совпадают.",
        "两次输入的密码不一致。",
        "兩次輸入的密碼不一致。",
    ],
    InitialKey => [
        "Initial gateway API key (optional)",
        "Начальный API-ключ шлюза (необязательно)",
        "初始网关 API Key（可选）",
        "初始閘道 API Key（選填）",
    ],
    GenerateKey => ["Leave empty to generate one", "Оставьте пустым для автоматической генерации", "留空自动生成", "留空自動產生"],
    Upstreams => ["Upstreams", "Вышестоящие сервисы", "上游渠道", "上游渠道"],
    UpstreamChoice => [
        "How do you want to add upstreams?",
        "Как добавить вышестоящие сервисы?",
        "要如何添加上游？",
        "要如何新增上游？",
    ],
    ChooseChannels => ["Choose channels", "Выбрать каналы", "选择渠道", "選擇渠道"],
    ImportFile => [
        "Import a configuration export",
        "Импортировать экспорт конфигурации",
        "导入配置导出文件",
        "匯入設定匯出檔案",
    ],
    Later => ["Later, in the console", "Позже, в консоли", "稍后在管理界面添加", "稍後在管理介面新增"],
    Channels => [
        "Channels you plan to use",
        "Каналы, которые вы собираетесь использовать",
        "打算使用的渠道",
        "打算使用的渠道",
    ],
    SelectHelp => [
        "↑↓ to move, enter to select",
        "↑↓ — перемещение, Enter — выбор",
        "↑↓ 移动，回车选择",
        "↑↓ 移動，Enter 選擇",
    ],
    ChannelsHelp => [
        "space to select, type to filter, enter to confirm",
        "пробел — выбрать, ввод текста — фильтр, Enter — подтвердить",
        "空格选择，输入文字筛选，回车确认",
        "空白鍵選擇，輸入文字篩選，Enter 確認",
    ],
    ImportPath => ["Configuration export file", "Файл экспорта конфигурации", "配置导出文件", "設定匯出檔案"],
    FileMissing => ["No such file.", "Файл не найден.", "文件不存在。", "檔案不存在。"],
    SourceKey => [
        "Master key of the exporting instance (optional)",
        "Мастер-ключ исходного экземпляра (необязательно)",
        "导出实例的主密钥（可选）",
        "匯出實例的主金鑰（選填）",
    ],
    SourceKeyHelp => [
        "only needed when the export carries sealed credential secrets",
        "нужен, только если экспорт содержит зашифрованные секреты учётных данных",
        "仅当导出文件包含加密的凭据密钥时需要",
        "僅當匯出檔案包含加密的憑據金鑰時需要",
    ],
    Starting => ["Setting up the instance…", "Настройка экземпляра…", "正在创建实例…", "正在建立實例…"],
    ExistingAdmin => [
        "This database already has users; its administrator was kept and no key was minted.",
        "В этой базе данных уже есть пользователи; администратор сохранён, новый ключ не создан.",
        "该数据库已有用户，已保留原有管理员，未生成新的 Key。",
        "該資料庫已有使用者，已保留原有管理員，未產生新的 Key。",
    ],
    Saved => ["Settings saved to {}.", "Настройки сохранены в {}.", "设置已保存到 {}。", "設定已儲存到 {}。"],
    Credentials => ["Credentials for {}", "Учётные данные для {}", "{} 的凭据", "{} 的憑據"],
    CredentialAction => ["Add a credential", "Добавить учётные данные", "添加凭据", "新增憑據"],
    PasteKey => ["Paste an API key", "Вставить API-ключ", "粘贴 API Key", "貼上 API Key"],
    SignIn => ["Sign in", "Войти", "登录账号", "登入帳號"],
    Skip => ["Skip", "Пропустить", "跳过", "略過"],
    ApiKey => ["API key", "API-ключ", "API Key", "API Key"],
    LaterInConsole => [
        "This channel takes {} credentials; add them later on the provider's page in the console.",
        "Этот канал использует учётные данные типа {}; добавьте их позже на странице провайдера в консоли.",
        "该渠道使用 {} 凭据，请稍后在管理界面的供应商页面添加。",
        "該渠道使用 {} 憑據，請稍後在管理介面的供應商頁面新增。",
    ],
    OpenLink => ["Open this link to sign in:", "Откройте ссылку для входа:", "打开以下链接登录：", "開啟以下連結登入："],
    PasteCallback => [
        "Paste the callback URL or code",
        "Вставьте URL обратного вызова или код",
        "粘贴回调 URL 或授权码",
        "貼上回呼 URL 或授權碼",
    ],
    DeviceCode => ["Then enter this code: {}", "Затем введите код: {}", "然后输入此代码：{}", "然後輸入此代碼：{}"],
    Waiting => ["Waiting for authorization…", "Ожидание авторизации…", "正在等待授权…", "正在等待授權…"],
    Denied => ["Authorization was denied.", "В авторизации отказано.", "授权被拒绝。", "授權被拒絕。"],
    Expired => ["The sign-in expired.", "Срок входа истёк.", "登录已过期。", "登入已過期。"],
    Added => ["Credential added.", "Учётные данные добавлены.", "凭据已添加。", "憑據已新增。"],
    Failed => ["That did not work: {}", "Не удалось: {}", "操作失败：{}", "操作失敗：{}"],
    Ready => ["The gateway is ready", "Шлюз готов к работе", "网关已准备就绪", "閘道已準備就緒"],
    BaseUrl => ["Client base URL", "Базовый URL для клиентов", "客户端 Base URL", "用戶端 Base URL"],
    GatewayKey => ["Gateway API key", "API-ключ шлюза", "网关 API Key", "閘道 API Key"],
    KeyShownOnce => [
        "The key is shown only now; save it before continuing.",
        "Ключ показывается только сейчас; сохраните его, прежде чем продолжить.",
        "Key 只显示这一次，请先保存再继续。",
        "Key 只顯示這一次，請先儲存再繼續。",
    ],
    Console => ["Console", "Консоль", "管理界面", "管理介面"],
    Service => [
        "Install as a background service that starts now and at login?",
        "Установить как фоновую службу, запускаемую сейчас и при входе в систему?",
        "安装为后台服务（立即启动，登录时自动启动）？",
        "安裝為背景服務（立即啟動，登入時自動啟動）？",
    ],
    Foreground => [
        "Serving in this terminal; press Ctrl+C to stop.",
        "Сервер работает в этом терминале; нажмите Ctrl+C для остановки.",
        "网关在此终端中运行，按 Ctrl+C 停止。",
        "閘道在此終端機中執行，按 Ctrl+C 停止。",
    ],
    Cancelled => ["Setup cancelled.", "Настройка отменена.", "已取消设置。", "已取消設定。"],
}

/// The label for one credential kind, as the console's `credentialKinds.*`.
pub fn credential_kind(lang: Lang, kind: &str) -> &str {
    match (kind, lang) {
        ("api_key", _) => "API Key",
        ("oauth", _) => "OAuth",
        ("cookie", _) => "Cookie",
        ("service_account", Lang::En) => "Service account",
        ("service_account", Lang::Ru) => "Сервисный аккаунт",
        ("service_account", Lang::ZhCn) => "服务账号",
        ("service_account", Lang::ZhTw) => "服務帳號",
        ("aws_access_key", Lang::En) => "AWS access key",
        ("aws_access_key", Lang::Ru) => "Ключ доступа AWS",
        ("aws_access_key", Lang::ZhCn) => "AWS 访问密钥对",
        ("aws_access_key", Lang::ZhTw) => "AWS 存取金鑰對",
        (other, _) => other,
    }
}
