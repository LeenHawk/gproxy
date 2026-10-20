---
title: "Облачное развёртывание"
description: "Развёртывание GPROXY на Cloudflare Workers, Netlify и Vercel, подключение базы данных и первый вход."
---

Информацию о собственных контейнерах для Northflank, Render и Heroku см. в [хостинг контейнеров](/ru/deployment/containers/).

GPROXY предоставляет шаблоны развертывания для Cloudflare Workers, Netlify и Vercel. Они загружают готовые пакеты выпусков, поэтому установка Rust не требуется. Консоль и API совместно используют ваш домен развертывания; консоль находится по адресу `/console/`.

<a id="choose-a-platform"></a>

## Выберите платформу

| Платформа | База данных платформы | Существующая база данных | WebSocket/Realtime в этом шаблоне |
| --- | --- | --- | --- |
| [Cloudflare Workers](#cloudflare-workers) | D1, созданный во время развертывания. | libSQL/Турсо | Поддерживается |
| [Netlify Functions](#netlify) | База данных Netlify | PostgreSQL | Не поддерживается |
| [Vercel Functions](#vercel) | Продукт PostgreSQL, выбранный во время развертывания | PostgreSQL | Поддерживается (бета-версия платформы) |

Для потоковой передачи HTTP и SSE вы можете использовать любой шаблон. Для WebSocket или Realtime выберите Cloudflare Workers, Vercel Functions (WebSocket Beta) или [CLI / развертывание контейнера](/ru/download/).

Cloudflare работает под управлением WASM; на двух других платформах используется собственный бессерверный исполняемый файл. Они используют общие средства управления GPROXY (API) и консоль, а типы баз данных, продолжительность функций и ограничения размера запросов зависят от платформы.

<a id="prepare-the-password-and-encryption-key"></a>

## Подготовьте пароль и ключ шифрования.

Каждой платформе необходимы эти две настройки:

| Параметр | Значение |
| --- | --- |
| `GPROXY_ADMIN_PASSWORD` | Пароль для входа длиной не менее 8 символов |
| `GPROXY_MASTER_KEY` | 32-байтовый ключ для шифрования учётных данных провайдера, закодированный как 64 шестнадцатеричных символа или base64 |

Вы можете сгенерировать мастер-ключ с помощью `openssl rand -hex 32`. Сохраните его и используйте то же значение для обновлений и повторных развертываний.

Имя пользователя по умолчанию — `admin`; установите `GPROXY_ADMIN_USER`, чтобы выбрать другой. Настройка пароля применяется при запуске: сначала обновляется пароль пользователя с тем же именем; если ни одно имя не совпадает, администратор `0` восстанавливается и получает настроенные имя и пароль. См. [настройка администратора и переопределение пароля](/ru/reference/configuration/#bootstrap).

Если вы используете существующую базу данных, вам также потребуются сведения о ее подключении. Шаги каждой платформы приведены ниже.

## Cloudflare Workers

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)][cloudflare-авто]

[Использовать внешнюю базу данных libSQL/Turso][cloudflare-external]

Используйте кнопку развертывания для D1 или ссылку на внешнюю базу данных для существующей базы данных libSQL/Turso.

1. Откройте выбранную вами запись и подключите свою учётную запись GitHub или GitLab.
2. Для D1 выберите имя базы данных; Cloudflare создает и связывает его. Для внешней базы данных введите `GPROXY_DATABASE_URL` и `GPROXY_LIBSQL_TOKEN`. Эта запись не создает D1.
3. Введите пароль администратора и мастер-ключ, сохраните `npm run build` и `npm run deploy` и выполните развертывание.

Turso URL может использовать `libsql://your-db.turso.io` или его эквивалент HTTPS. Это соединение использует интерфейс HTTP libSQL, а не строку подключения PostgreSQL.

Существующее развертывание D1 также может переключиться на libSQL с помощью этих переменных. `GPROXY_DATABASE_URL` имеет приоритет над D1. При изменении соединения выбирается другая база данных; он не переносит существующие данные.

Для развертывания Wrangler или пользовательской конфигурации Workers см. [развертывание Cloudflare вручную](#manual-cloudflare-deployment) ниже.

## Netlify

[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)][netlify-авто]

[Использовать существующую базу данных PostgreSQL][netlify-external]

Используйте кнопку развертывания для базы данных Netlify или ссылку на внешнюю базу данных для существующей базы данных PostgreSQL.

1. Откройте выбранную вами запись, авторизуйте репозиторий и введите пароль администратора и мастер-ключ.
2. Для базы данных Netlify шаблон считывает соединение с платформой; строка подключения вручную не требуется. Для существующей базы данных введите строку подключения к PostgreSQL как `GPROXY_DATABASE_URL`.
3. Сохраните команду сборки шаблона и конфигурацию функций, а затем разверните.

Внешнее подключение выглядит как `postgresql://user:password@db.example.com/gproxy?sslmode=require`. Используйте фактические данные подключения, предоставленные вашей службой базы данных.

Обе записи используют один и тот же шаблон Netlify. Netlify по-прежнему может предоставлять базу данных платформы, когда вы предоставляете внешнее соединение; GPROXY дает приоритет `GPROXY_DATABASE_URL`.

## Vercel

[![Deploy with Vercel](https://vercel.com/button)][vercel-авто]

[Использовать существующую базу данных PostgreSQL][vercel-external]

Используйте кнопку развертывания, если вам нужна новая база данных, или ссылку на внешнюю базу данных для существующей базы данных PostgreSQL.

1. Откройте выбранную вами запись. Для базы данных платформы выберите и прикрепите продукт PostgreSQL при появлении соответствующего запроса. Запись внешней базы данных не требует установки продукта базы данных.
2. Введите пароль администратора и мастер-ключ. Для существующей базы данных также установите `GPROXY_DATABASE_URL`.
3. Сохраните команду сборки шаблона и `vercel.json`, а затем разверните.

Соединения платформы считываются из `DATABASE_URL` или `POSTGRES_URL`. Если ваш продукт использует другое имя переменной, поместите ее строку подключения в `GPROXY_DATABASE_URL`.

В этом шаблоне используются **функции Node.js**. Не переключайте его на Edge Runtime.

<a id="sign-in-and-make-a-request"></a>

## Войдите и сделайте запрос

После развертывания откройте `https://your-domain/console/` и войдите в систему, используя `admin` (или выбранное вами имя пользователя) и настроенный пароль.

Добавьте провайдера и учётные данные в консоли. Проверьте учётные данные, создайте маршрут модели и ключ API шлюза, затем следуйте [Ваш первый запрос](/ru/getting-started/first-request/). Клиенты используют ваше развертывание URL, например `https://gateway.example.com/v1`, и проходят аутентификацию с помощью ключа API шлюза.

Если Netlify или Vercel возвращает 503, проверьте журналы функций и убедитесь, что база данных подключена, ее строка подключения работает, а пароль и мастер-ключ установлены. Учётной записи PostgreSQL также необходимо разрешение на создание схемы `gproxy` и таблиц приложений.

<a id="updates-and-runtime-limits"></a>

## Обновления и ограничения времени выполнения

Конфигурация, учётные записи и использование хранятся в базе данных. Сохраняйте базу данных и мастер-ключ при обновлении. Шаблоны по умолчанию загружают последнюю стабильную версию и проверяют SHA-256. Очистите кэш сборки платформы и повторно разверните ее для обновления. Установите для переменной среды сборки `GPROXY_RELEASE_VERSION` тег выпуска только в том случае, если вы хотите закрепить версию. Релиз должен включать соответствующие пакеты платформы. При размещенных развертываниях не используется встроенное в консоль средство обновления двоичных файлов.

Все шаблоны поддерживают HTTP и SSE; Netlify Functions отклоняет обновления WebSocket. Vercel использует платформу [Вебсокет бета](https://vercel.com/docs/functions/websockets). Ограничения продолжительности функций или вытеснение экземпляра могут по-прежнему отключать WebSockets, поэтому клиенты должны обрабатывать повторное подключение. Длительный вывод, большие файлы и высокая степень параллелизма по-прежнему зависят от ограничений платформы. Проверьте текущие разрешения [Netlify Функции](https://docs.netlify.com/build/functions/overview/) и [Vercel Функции](https://vercel.com/docs/functions/limitations) для вашей рабочей нагрузки.

Эти два шаблона не настраивают хранилище файлов. Для S3/R2 или общедоступных файлов используйте специальную сборку Workers, как описано ниже, или CLI. В Netlify и Vercel `GPROXY_PUBLIC_BASE_URL` исправляет общедоступный URL. Без него Netlify и Vercel используют производственный URL, предоставляемый платформой; в других местах конечные точки OAuth отвечают с хоста каждого запроса, а ссылки на публикации остаются отключенными.

<a id="manual-cloudflare-deployment"></a>

## Ручное развертывание Cloudflare

<a id="use-a-release-bundle"></a>

### Используйте пакет выпуска

Загрузите `gproxy-edge-cloudflare.zip`, распакуйте его и войдите в каталог `cloudflare`. Он содержит Worker, консоль и `wrangler.toml`.

Для D1 установите зависимости и создайте базу данных:

```sh
pnpm install
pnpm exec wrangler d1 create gproxy
```

Поместите возвращенную базу данных ID в `wrangler.toml`, сохранив имя привязки `DB`:

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "replace-with-your-database-id"
```

Установите мастер-ключ и пароль администратора, затем выполните развертывание:

```sh
pnpm exec wrangler secret put GPROXY_MASTER_KEY
pnpm exec wrangler secret put GPROXY_ADMIN_PASSWORD
pnpm exec wrangler deploy --dry-run
pnpm exec wrangler deploy
```

Для libSQL установите `GPROXY_DATABASE_URL` и `GPROXY_LIBSQL_TOKEN` и удалите неиспользуемую привязку D1. Пакет выпуска включает libSQL; сохраните эту функцию при создании собственного. GPROXY создает свои таблицы при первом запуске, поэтому отдельная миграция Wrangler SQL не требуется.

<a id="workers-configuration-and-static-assets"></a>

### Конфигурация Workers и статические ресурсы

`GPROXY_CONFIG` принимает документ конфигурации JSON. Именованные секреты переопределяют соответствующие поля. В этом примере используется D1 и устанавливается общедоступный URL:

```toml
[vars]
GPROXY_CONFIG = '{"store":{"kind":"d1","binding":"DB"},"cache":{"kind":"store"},"public_base_url":"https://gateway.example.workers.dev"}'
```

Workers Assets обслуживает консоль. Сохраните конфигурацию маршрутизации шаблона, чтобы `/console/` использовал статические ресурсы, а другие запросы достигали шлюза:

```toml
[assets]
directory = "./public"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/*", "!/", "!/console", "!/console/*"]
```

Workers поддерживает D1 или libSQL без локальных баз данных SQLite, TCP или локальных файловых каталогов. Для S3/R2 включите функцию `s3` во время сборки и установите `GPROXY_S3_ACCESS_KEY_ID`, `GPROXY_S3_SECRET_ACCESS_KEY` и конфигурацию хранилища файлов. См. поля [ссылка на конфигурацию](/ru/reference/configuration/).

<a id="build-from-source"></a>

### Сборка из исходного кода

Запускаем из корня репозитория:

```sh
rustup target add wasm32-unknown-unknown
cargo install worker-build
pnpm --dir console install --frozen-lockfile
pnpm --dir deploy/cloudflare install
pnpm --dir deploy/cloudflare build
pnpm --dir deploy/cloudflare check
```

См. [Сборка из исходников](/ru/deployment/release-build/) для вариантов сборки и упаковки.

[cloudflare-auto]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-button
[cloudflare-external]: https://deploy.workers.cloudflare.com/?url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fcloudflare-external
[netlify-auto]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless
[netlify-external]: https://app.netlify.com/start/deploy?repository=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy&branch=dev&create_from_path=deploy%2Fserverless#GPROXY_DATABASE_URL=
[vercel-auto]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY&products=%5B%7B%22type%22%3A%22integration%22%2C%22group%22%3A%22postgres%22%2C%22protocol%22%3A%22storage%22%7D%5D
[vercel-external]: https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FLeenHawk%2Fgproxy%2Ftree%2Fdev%2Fdeploy%2Fserverless&project-name=gproxy&repository-name=gproxy&env=GPROXY_ADMIN_PASSWORD%2CGPROXY_MASTER_KEY%2CGPROXY_DATABASE_URL
