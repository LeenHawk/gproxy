---
title: "Сборка из исходного кода"
description: "Сборка CLI, приложения, Worker и консоли GPROXY v4 и запуск проверок CI."
---

Рабочий процесс выпуска публикует отправки `dev` в `nightly` (канал обновлений для разработчиков), а `main` отправляет в `staging` (канал обновлений бета-версии). Стабильные версии выпускаются из `main` и обновляют как выпуск, так и бета-версию; они никогда не перезаписывают dev. Версионные выпуски создаются на основе тегов `v*`, соответствующих версии рабочей области. Инструкции ниже охватывают сборки исходного кода; `.github/workflows/release.yml` управляет автоматизированной упаковкой.

Прежде чем отправлять dev, запустите `bash scripts/push-dev.sh`, чтобы выполнить перебазировку на последнюю версию main. Включите локальную защиту от нажатия с помощью `git config core.hooksPath .githooks`. Публичные вложения содержат пакеты `manifest.json` и `SHA256SUMS`; отдельные файлы контрольных сумм и происхождение сборки остаются внутренними входами CI.

<a id="release-packages"></a>

## Пакеты релизов

CLI (`gproxy-*`) и Приложение (`gproxy-tauri-*`) — отдельные программы; у каждого есть свои портативные архивы и установщики. Приложения встраивают консоль.

| Платформа | CLI | Приложение |
| --- | --- | --- |
| Linux GNU (x86_64, aarch64, riscv64) | ZIP, DEB | ZIP, DEB |
| Linux musl (x86_64, aarch64, riscv64) | ZIP, Альпийский APK | ZIP, Альпийский APK |
| Windows (x86_64, аарх64) | ZIP, MSIX | ZIP, MSIX |
| macOS (x86_64, aarch64) | ZIP, DMG | ZIP, DMG |
| Android (x86_64, aarch64) | ZIP, Термукс DEB | только APK |
| OpenHarmony / HarmonyOS NEXT | ARM64 / x86_64 ZIP | ARM64 беззнаковый HAP |

Linux CLI DEB устанавливает `gproxy` под `/usr/bin`; Termux DEB устанавливается под `/data/data/com.termux/files/usr`. В сборках Android CLI используется исходный код `distribution/termux/` и закрепленный официальный сборщик Termux. И DEB, и ZIP используют сертификаты Termux `libc++`, OpenSSL и CA; ZIP-файлы включают программу запуска, исполняемый файл и `TERMUX.txt` с инструкциями по установке/обновлению. В этих сборках отключено самообновление: установите более новую версию DEB с APT или используйте `pkg upgrade gproxy`, как только включенный репозиторий предоставит ее. Windows CLI MSIX использует отдельный идентификатор `.CLI` и псевдоним выполнения консоли. DMG для macOS CLI включают исполняемый файл и инструкции по установке терминала. ZIP-архивы приложений сохраняют ресурсы рабочего стола, а в macOS — полный `.app`.

В сборках OHOS используется кэшированный образ цепочки инструментов и закрепленная экспериментальная ветка Tauri; на других платформах сохраняется стабильная версия Tauri. Файлы HAP требуют подписи перед установкой. HAP был проверен на эмуляторе. Реализовано непрерывное фоновое выполнение, но доступность зависит от устройства, версии OS и предоставленных разрешений; прокси-сервер останавливается при выходе из процесса приложения.

Ночные имена файлов остаются фиксированными; в манифесте записан коммит SHA. GNU Для приложений Linux требуются пакеты GTK 3 и WebKitGTK 4.1. musl собирает целевой Alpine и поставляет его в виде APK, а не DEB. macOS использует специальную подпись; Подписание и нотариальное заверение разработчика ID не настроены.

Windows использует идентификатор пакета Microsoft Store. Настройте четыре переменные в среде `release`: `MS_STORE_IDENTITY_NAME`, `MS_STORE_DISPLAY_NAME`, `MS_STORE_IDENTITY_PUBLISHER` и `MS_STORE_PUBLISHER_DISPLAY_NAME`. Стабильные выпуски сохраняют оба пакета архитектуры для отправки в Магазин. Если `MS_STORE_PUBLISH_ENABLED` включен, успешная публикация GitHub запускает рабочий процесс отправки в Магазин. Файлы MSIX на GitHub не подписаны, как и пакеты отправки в Магазин; Магазин подписывает их для распространения. Это не доверенные пакеты, которые можно установить двойным щелчком мыши. Приложению требуется системная среда выполнения WebView2.

APK-файлы Android подписываются и проверяются с использованием существующих секретов `ANDROID_SIGNING_*`. Запись `<target-triple>-tauri-apk` каждого приложения присоединяется к манифесту обновления, подписанному Ed25519, отдельно от обновлений CLI ZIP. Устаревшая серверная оболочка APK больше не создается. При отсутствии необходимых ключей или переменных идентификатора магазина не удается выполнить упаковку.

Для локальной сборки подготовьте консоль и вызовите сценарий упаковки:

```sh
pnpm --dir console build
node console/scripts/sync-to-embed.mjs
pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
TARGET_OS=linux TARGET_TRIPLE=x86_64-unknown-linux-gnu \
  ARTIFACT_NAME=gproxy-tauri-linux-x86_64 \
  GPROXY_BUILD_VERSION=$(scripts/release-metadata.sh version) \
  scripts/package-tauri-release.sh
```

Выходы идут на `dist/release/`. Рабочий процесс также публикует собственные ZIP-файлы сервера, пакеты Termux, пакеты Edge и образы контейнеров GNU/musl. Пакеты приложений получают сертификаты происхождения сборки, размещенные на GitHub.

<a id="build-environment"></a>

## Создание среды

Установите стабильную версию Rust (выпуск 2024 г.), Go, Clang, Node.js 22.12+ (рекомендуется 24 LTS) и pnpm. Для настольных сборок Linux также требуются пакеты разработки `webkit2gtk-4.1`, `gtk+-3.0` и `libsoup-3.0`. Рабочий процесс выпуска настраивает другие цепочки инструментов платформы.

<a id="build-the-cli-and-console"></a>

## Соберите CLI и консоль.

Из корня репозитория:

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build
cargo build -p gproxy --release
./target/release/gproxy serve --data-dir ./data
```

Сборка консоли копирует ресурсы в каталоги встраивания HTTP и Tauri. Последующая сборка Rust включает их. Новая проверка, созданная только с помощью Rust, не имеет ресурсов консоли, поэтому `/console` возвращает 404. Вы также можете передать `--console-path console/dist` для обслуживания отдельной сборки.

Значения по умолчанию CLI включают все каналы, кэш памяти, локальное хранилище файлов и связанный словарь. Выберите меньше каналов или добавьте серверные части по мере необходимости:

```sh
cargo build -p gproxy --release --no-default-features \
  --features embedded-console,memory,fs,codex,claudecode,openai,custom
```

PostgreSQL, MySQL, Redis и S3 требуют `postgres`, `mysql`, `redis` и `s3` соответственно. SQLite всегда доступен.

<a id="headless-cli-for-ci"></a>

### Безголовый CLI для CI

Рабочий процесс выпуска создает автономные пакеты независимо от внешнего интерфейса:

| Платформа | Архитектуры | Пакет |
| --- | --- | --- |
| Линукс GNU | x86_64, aarch64, riscv64 | `gproxy-headless-linux-<arch>.zip` |
| Linux musl | x86_64, aarch64, riscv64 | `gproxy-headless-linux-<arch>-musl.zip`, `gproxy-headless-linux-<arch>-musl.apk` |
| Окна | x86_64, аарх64 | `gproxy-headless-windows-<arch>.zip` |
| macOS | x86_64, аарх64 | `gproxy-headless-macos-<arch>.zip` |
| Андроид (Термукс) | x86_64, аарх64 | `gproxy-headless-android-<arch>.zip`, `gproxy-headless-android-<arch>.deb` |

Загрузите с [Релизы](https://github.com/LeenHawk/gproxy/releases): `nightly` для разработчиков, `staging` для бета-версии или стабильную версию. Самообновление выбирает соответствующий безголовый пакет. На Android установите пакет Termux с `pkg install ./gproxy-headless-android-<arch>.deb` и запустите `gproxy serve --console=false`. Он заменяет полный пакет `gproxy`, поскольку оба предоставляют команду `gproxy`. Чтобы вместо этого использовать ZIP, запустите `pkg install libc++ openssl ca-certificates`, распакуйте его в домашний каталог Termux, затем запустите `./gproxy serve --console=false`. В Windows используйте `gproxy.exe` вместо `gproxy`.

Этот вариант исключает встроенную веб-консоль и сохраняет прокси-маршруты, API управления, все каналы, SQLite, кэш памяти, локальное хранилище файлов и встроенный словарь. Настройте его через CLI или управление API. Для сборки не нужны библиотеки Node.js, pnpm или настольные системы:

```sh
cargo build --locked --release -p gproxy --bin gproxy \
  --no-default-features --features channels,memory,fs,bundled-vocabulary,postgres,mysql
./target/release/gproxy serve --console=false
```

Если пропустить `embedded-console`, будут исключены ресурсы внешнего интерфейса, даже если оформление заказа уже содержит встроенную консоль. `--console=false` также отключает обслуживание каталога внешней консоли. Чтобы создать консольный вариант с настраиваемым набором функций, добавьте `embedded-console`.

<a id="build-application"></a>

## Создать приложение

После сборки консоли:

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

Новый экземпляр открывает мастер установки. Окно управляет им через IPC, а прослушиватель HTTP обслуживает клиентов шлюза. Используйте цепочки инструментов платформы рабочего процесса выпуска и сценарии упаковки для мобильных сборок.

Члены рабочей области по умолчанию исключают Tauri, поэтому сборки серверной части не требуют зависимостей рабочего стола. `--workspace` включает его.

<a id="build-workers"></a>

## Сборка Workers

Из корня репозитория:

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

`worker-build` компилирует и оптимизирует WASM. `check` выполняет пробное развертывание Wrangler без публикации. См. [Периферийное развертывание](/ru/deployment/edge/) для получения информации о конфигурации и ограничениях времени выполнения.

<a id="validate-changes"></a>

## Подтвердить изменения

Члены бэкэнда по умолчанию:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Проверьте хост рабочего стола отдельно:

```sh
cargo clippy -p gproxy-host-tauri --all-targets -- -D warnings
cargo test -p gproxy-host-tauri
```

Проверки WASM включают поддерживаемые библиотеки и хосты, а не CLI или Tauri. Не используйте всю рабочую область для этой цели:

```sh
cargo check --target wasm32-unknown-unknown \
  -p gproxy-protocol -p gproxy-store -p gproxy-seaorm -p gproxy-client \
  -p gproxy-cache -p gproxy-channel -p gproxy-core -p gproxy-file \
  -p gproxy-tokenizer -p gproxy-sdk -p gproxy-app -p gproxy-host-axum \
  -p gproxy-host-edge
```

Консоль и документация:

```sh
pnpm --dir console lint
pnpm --dir console test
pnpm --dir console build
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs check
pnpm --dir docs build
bash scripts/check-docs.sh
```

После изменения DTO Rust запустите `pnpm --dir console types`, чтобы восстановить типы TypeScript.

<a id="publish"></a>

## Публиковать

Запустите `bash scripts/release.sh` из чистой проверки, включенной в `main`. Прежде чем отправить стабильный тег, он обновляет зависимости консоли и документации npm с помощью `pnpm update --latest`, обновляет требования к реестру Rust с помощью `cargo upgrade` (включая основные и закрепленные версии) и обновляет `Cargo.lock` с помощью `cargo update`. Мосты совместимости для конфигураций RSA/getrandom, JNI/Tauri, Codex reqwest и WebSocket сохраняют строки версий, требуемые провайдером; совместимые обновления по-прежнему обновляются через `cargo update`. Сначала установите помощник Rust с `cargo install cargo-edit --locked`. Если эти команды изменяют отслеживаемые файлы, сценарий останавливается перед созданием или отправкой тега. Устраните изменения совместимости, запустите указанные выше проверки, зафиксируйте и отправьте `main`, затем перезапустите сценарий. Предварительные теги пропускают это обновление зависимостей.

Тег версии должен соответствовать версии рабочей области и иметь файл `docs/release-notes/v<version>.md`. Рабочий процесс собирает пакеты, создает подписанный манифест обновления и загружает ресурсы. Ошибка сборки или подписи означает, что публикация неполная.

Теги версий также вызывают `scripts/publish-crates.sh` для выбранных библиотек MIT. Другие ящики используют зависимости git или path; см. [Встраивание ядра](/ru/reference/embedding/).
