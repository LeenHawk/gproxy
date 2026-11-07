# Contributing to GPROXY / 贡献指南 / Участие в разработке

[English](#english) · [简体中文](#简体中文) · [Русский](#русский)

---

## English

Bug reports, fixes, new channels, translations and documentation fixes are all
welcome. By taking part you agree to follow the [Code of
Conduct](CODE_OF_CONDUCT.md).

### Where to go

Security vulnerabilities must not be reported in public; follow
[SECURITY.md](SECURITY.md) instead.

Questions about configuration, deployment or usage belong in
[Discussions](https://github.com/LeenHawk/gproxy/discussions/categories/q-a).
Bugs and feature requests go to
[Issues](https://github.com/LeenHawk/gproxy/issues); please search first. If
you would rather not post something publicly, email <leenhawk@leenhawk.com>.
Accessibility problems are bugs too; [ACCESSIBILITY.md](ACCESSIBILITY.md)
explains how to report them.

Open an issue before starting on anything large, such as a new channel, a new
protocol, a schema change or a new console page, so that the design can be
agreed before you write a lot of code. A new upstream channel follows [Adding
a channel](https://gproxy.leenhawk.com/development/adding-a-channel/). If you
would rather not have your channel in this repository, you can ship it from
your own crate; the same page explains how.

### Branches

`main` holds the released version and receives bug fixes; pushes to it
publish to the beta update channel. `dev` is where new features and the next
major version are developed; pushes to it publish to the nightly channel. Base
a bug fix for the current stable release on `main`, and anything that adds a
feature or changes behaviour on `dev`.

`dev` is rebased onto `main` regularly, so keep your branch up to date by
rebasing rather than merging. Keep each pull request to one change.

### Setting up

Native builds need stable Rust, Go and Clang. The console and the docs need
Node.js 22.12 or later (24 LTS recommended) and pnpm. Linux desktop builds
also need the development packages for WebKitGTK 4.1, GTK 3 and libsoup 3.

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build      # also syncs the embedded console assets
cargo run -p gproxy -- serve
```

Test against a fresh data directory. Do not point a development instance at a
production database or give it production upstream credentials.

### Checks

CI runs all of the following. Before opening a pull request, run the ones that
your change touches:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm --dir console lint       # includes TypeScript, ESLint and i18n parity
pnpm --dir console test
git submodule update --init docs && pnpm --dir docs install && pnpm --dir docs check
```

Desktop, mobile and WebAssembly builds are described in [Building from
source](https://gproxy.leenhawk.com/development/building/).

### Guidelines

Follow the style of the code around your change: its naming, how much it
comments and the idioms it uses. Keep the diff small and leave unrelated
formatting alone.

A bug fix should come with a regression test where that is practical. Do not
commit scratch tests or one-off reproductions.

Console strings live in `console/src/locales/` (`en`, `ru`, `zh-CN`, `zh-TW`).
A new key has to be added in all four; `pnpm --dir console lint` checks this.

A change that users can see should also update the documentation, in English
and in `zh-cn/`, `zh-tw/` and `ru/`. The documentation lives in `docs/`, which
is the [gproxy-docs](https://github.com/LeenHawk/gproxy-docs) submodule, so
open the docs change in that repository.

If you change a DTO that is exported to TypeScript, run
`pnpm --dir console types` and commit the result.

Never commit API keys, OAuth tokens, cookies, account identifiers or
unredacted upstream responses. That includes tests, fixtures, screenshots and
issue comments.

### Commit messages

Commit messages follow [Conventional
Commits](https://www.conventionalcommits.org/):

```text
<type>(<optional scope>): <summary in imperative mood>
```

The usual types are `feat`, `fix`, `docs`, `ci`, `build`, `refactor`, `perf`,
`test` and `chore`. For example:

```text
fix(claude): keep the signature on empty thinking blocks
feat(console): add a credential health filter
```

### Pull requests

1. Fork the repository and branch from `main` for a bug fix or from `dev` for
   a feature.
2. Make the change, including tests and documentation.
3. Run the checks above.
4. Open the pull request against the same branch and fill in the template.

The maintainer may ask for changes, and may squash your commits or rebase your
branch when merging.

### License

GPROXY is licensed under the AGPL-3.0-or-later. The crates `gproxy-protocol`,
`gproxy-protocol-macros`, `gproxy-client`, `gproxy-client-sdk`,
`gproxy-cache`, `gproxy-file`, `gproxy-seaorm`, `gproxy-store` and
`gproxy-tokenizer` are MIT-licensed. By submitting a
contribution you agree that it is licensed under the license of the files or
crate it changes.

---

## 简体中文

欢迎提交问题反馈、修复、新渠道、翻译和文档修正。参与本项目即表示你同意遵守[行为准则](CODE_OF_CONDUCT.md)。

### 问题发到哪里

安全漏洞不要公开报告，请按 [SECURITY.md](SECURITY.md) 的方式联系。

配置、部署和使用上的问题请发到 [Discussions](https://github.com/LeenHawk/gproxy/discussions/categories/q-a)。Bug 和功能建议提交到 [Issues](https://github.com/LeenHawk/gproxy/issues)，提交前请先搜索有没有相同的问题。不方便公开的内容可以发邮件到 <leenhawk@leenhawk.com>。无障碍方面的问题同样算作 Bug，报告方式见 [ACCESSIBILITY.md](ACCESSIBILITY.md)。

新渠道、新协议、数据库结构变更、控制台新页面这类较大的改动，请先开 Issue 把方案讨论清楚，再开始写大量代码。新增上游渠道的步骤见[添加渠道](https://gproxy.leenhawk.com/zh-cn/development/adding-a-channel/)；不想合进本仓库的渠道也可以放在你自己的 crate 里发布，同一页有说明。

### 分支

`main` 是已发布的版本，接收 Bug 修复，推送后发布到 beta 更新通道。`dev` 用来开发新功能和下一个大版本，推送后发布到 nightly 通道。修复当前稳定版的 Bug 请基于 `main`；新增功能或改变现有行为的改动请基于 `dev`。

`dev` 会定期 rebase 到 `main` 上，所以同步你的分支时请用 rebase，不要用 merge。每个 Pull Request 只做一件事。

### 开发环境

原生构建需要 stable Rust、Go 和 Clang。控制台和文档需要 Node.js 22.12 或更高版本（推荐 24 LTS）以及 pnpm。在 Linux 上构建桌面端，还需要 WebKitGTK 4.1、GTK 3 和 libsoup 3 的开发包。

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build      # 同时同步内嵌的控制台资源
cargo run -p gproxy -- serve
```

本地测试请使用全新的数据目录。不要让开发实例连接生产数据库，也不要给它使用生产环境的上游凭证。

### 检查

以下检查 CI 都会运行。提交 Pull Request 之前，请至少运行与你的改动有关的几项：

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm --dir console lint       # 包括 TypeScript、ESLint 和多语言一致性检查
pnpm --dir console test
git submodule update --init docs && pnpm --dir docs install && pnpm --dir docs check
```

桌面端、移动端和 WebAssembly 的构建见[从源码构建](https://gproxy.leenhawk.com/zh-cn/development/building/)。

### 约定

代码风格跟随周围的代码，包括命名、注释的多少和惯用写法。改动尽量小，不要顺手调整无关的格式。

修复 Bug 时，能写回归测试的话请一并提交。临时测试和一次性的复现代码不要提交。

控制台文案在 `console/src/locales/` 下（`en`、`ru`、`zh-CN`、`zh-TW`），新增的键四种语言都要加，`pnpm --dir console lint` 会检查。

用户能感知到的改动需要同步更新文档，英文以及 `zh-cn/`、`zh-tw/`、`ru/` 都要改。文档在 `docs/` 目录，它是 [gproxy-docs](https://github.com/LeenHawk/gproxy-docs) 子模块，文档改动请提交到那个仓库。

修改了导出到 TypeScript 的 DTO 时，请运行 `pnpm --dir console types` 并提交生成的文件。

不要提交 API Key、OAuth Token、Cookie、账号标识或未脱敏的上游响应。测试、fixture、截图和 Issue 评论里也一样。

### 提交信息

提交信息使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 格式，摘要用英文祈使句：

```text
<type>(<可选 scope>): <summary in imperative mood>
```

常用的类型有 `feat`、`fix`、`docs`、`ci`、`build`、`refactor`、`perf`、`test` 和 `chore`。例如：

```text
fix(claude): keep the signature on empty thinking blocks
feat(console): add a credential health filter
```

### Pull Request

1. Fork 本仓库。修 Bug 从 `main` 建分支，加功能从 `dev` 建分支。
2. 完成改动，包括测试和文档。
3. 运行上面的检查。
4. 向对应的分支提交 Pull Request，并填写模板。

维护者可能会要求修改，合并时也可能压缩你的提交或 rebase 你的分支。

### 许可证

GPROXY 采用 AGPL-3.0-or-later 许可证；`gproxy-protocol`、`gproxy-protocol-macros`、`gproxy-client`、`gproxy-client-sdk`、`gproxy-cache`、`gproxy-file`、`gproxy-seaorm`、`gproxy-store` 和 `gproxy-tokenizer` 这几个 crate 采用 MIT 许可证。提交贡献即表示你同意这份贡献按它所修改的文件或 crate 的许可证授权。

---

## Русский

Мы принимаем сообщения об ошибках, исправления, новые каналы, переводы и
правки документации. Участвуя в проекте, вы соглашаетесь соблюдать [Кодекс
поведения](CODE_OF_CONDUCT.md).

### Куда обращаться

Об уязвимостях нельзя сообщать публично; порядок описан в
[SECURITY.md](SECURITY.md).

Вопросы о настройке, развёртывании и использовании задавайте в
[Discussions](https://github.com/LeenHawk/gproxy/discussions/categories/q-a).
Ошибки и предложения отправляйте в
[Issues](https://github.com/LeenHawk/gproxy/issues), предварительно поискав
похожие. Если что-то не хочется публиковать, напишите на
<leenhawk@leenhawk.com>. Проблемы доступности тоже считаются ошибками; как о
них сообщать, написано в [ACCESSIBILITY.md](ACCESSIBILITY.md).

Прежде чем браться за крупное изменение (новый канал, новый протокол,
изменение схемы базы данных, новую страницу консоли), откройте issue и
согласуйте подход, пока не написано много кода. Новый канал добавляется по
руководству [Добавление
канала](https://gproxy.leenhawk.com/ru/development/adding-a-channel/). Канал,
который вы не хотите включать в этот репозиторий, можно поставлять из
собственного крейта; это описано там же.

### Ветки

В `main` находится выпущенная версия, туда попадают исправления ошибок;
отправка в неё публикует сборку в бета-канале обновлений. В `dev`
разрабатываются новые возможности и следующая мажорная версия; отправка в неё
публикует сборку в канале nightly. Исправление ошибки текущей стабильной
версии делайте от `main`, а новые возможности и изменения поведения — от `dev`.

`dev` регулярно перебазируется на `main`, поэтому обновляйте свою ветку
через rebase, а не merge. Один pull request — одно изменение.

### Среда разработки

Для нативной сборки нужны стабильный Rust, Go и Clang. Для консоли и
документации — Node.js 22.12 или новее (рекомендуется 24 LTS) и pnpm. Для
сборки настольного приложения в Linux также нужны пакеты разработки WebKitGTK
4.1, GTK 3 и libsoup 3.

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build      # заодно синхронизирует встроенные файлы консоли
cargo run -p gproxy -- serve
```

Для проверки используйте новый каталог данных. Не подключайте
разработческий экземпляр к рабочей базе данных и не давайте ему рабочие
учётные данные провайдеров.

### Проверки

CI выполняет все перечисленные ниже проверки. Перед отправкой pull request
запустите те, которые затрагивает ваше изменение:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm --dir console lint       # включает TypeScript, ESLint и проверку переводов
pnpm --dir console test
git submodule update --init docs && pnpm --dir docs install && pnpm --dir docs check
```

Сборка настольных и мобильных приложений и WebAssembly описана в разделе
[Сборка из исходного кода](https://gproxy.leenhawk.com/ru/development/building/).

### Правила

Пишите в стиле окружающего кода: те же имена, та же плотность комментариев,
те же приёмы. Делайте изменения небольшими и не трогайте форматирование там,
где вы ничего не меняли.

К исправлению ошибки по возможности прилагайте регрессионный тест. Черновые
тесты и одноразовые воспроизведения не коммитьте.

Строки консоли лежат в `console/src/locales/` (`en`, `ru`, `zh-CN`, `zh-TW`).
Новый ключ нужно добавить во все четыре файла; `pnpm --dir console lint` это
проверяет.

Если изменение заметно пользователю, обновите и документацию: английскую
версию, а также `zh-cn/`, `zh-tw/` и `ru/`. Документация находится в `docs/`,
это подмодуль [gproxy-docs](https://github.com/LeenHawk/gproxy-docs), поэтому
изменения документации отправляйте в тот репозиторий.

Если вы изменили DTO, экспортируемый в TypeScript, выполните
`pnpm --dir console types` и закоммитьте результат.

Никогда не коммитьте API-ключи, токены OAuth, cookie, идентификаторы учётных
записей и неочищенные ответы провайдеров — ни в тестах, ни в фикстурах, ни на
скриншотах, ни в комментариях к issue.

### Сообщения коммитов

Сообщения коммитов следуют [Conventional
Commits](https://www.conventionalcommits.org/ru/), краткое описание пишется
по-английски в повелительном наклонении:

```text
<type>(<optional scope>): <summary in imperative mood>
```

Обычные типы: `feat`, `fix`, `docs`, `ci`, `build`, `refactor`, `perf`, `test`
и `chore`. Например:

```text
fix(claude): keep the signature on empty thinking blocks
feat(console): add a credential health filter
```

### Pull request

1. Сделайте форк репозитория и создайте ветку от `main` для исправления ошибки
   или от `dev` для новой возможности.
2. Внесите изменение вместе с тестами и документацией.
3. Запустите проверки, перечисленные выше.
4. Откройте pull request в ту же ветку и заполните шаблон.

Сопровождающий может попросить доработок, а при слиянии — объединить ваши
коммиты или перебазировать ветку.

### Лицензия

GPROXY распространяется по лицензии AGPL-3.0-or-later. Крейты
`gproxy-protocol`, `gproxy-protocol-macros`, `gproxy-client`,
`gproxy-client-sdk`, `gproxy-cache`, `gproxy-file`, `gproxy-seaorm`,
`gproxy-store` и `gproxy-tokenizer` распространяются по лицензии MIT. Отправляя вклад, вы соглашаетесь, что он лицензируется на условиях
лицензии тех файлов или крейта, которые он изменяет.
