# Accessibility / 无障碍 / Доступность

[English](#english) · [简体中文](#简体中文) · [Русский](#русский)

---

## English

### Commitment

GPROXY aims for everyone to be able to set up and operate their gateway,
including people who use a keyboard, screen reader, magnification, high
contrast or larger text. We target
[WCAG 2.2](https://www.w3.org/TR/WCAG22/) Level AA for the interfaces below and
treat accessibility barriers as bugs.

This is a goal, not a certification: GPROXY has not been audited by a third
party.

### Scope

- The management console and user portal, in a browser or embedded in the
  desktop and mobile applications
- The setup wizard, sign-in and OAuth consent pages
- The documentation site at <https://gproxy.leenhawk.com/>

The gateway API itself has no user interface. Sign-in pages of upstream
providers are outside the project's control.

### What is in place

- Full keyboard operation, a "skip to main content" link and a focus outline
  that stays visible, including in Windows high contrast (forced colors) mode
- Focus returns to the control that opened a dialog when it closes
- Labelled controls, with form errors and descriptions linked to their fields
- A distinct page title for each page, and the current page marked in navigation
- Wide tables are scrollable regions reachable by keyboard
- Charts come with an equivalent data table
- Light and dark themes, reduced-motion support, and English, Simplified
  Chinese, Traditional Chinese and Russian interfaces
- Desktop applications support the standard zoom shortcuts; the Android
  application follows the system font size and supports pinch-to-zoom
- Automated accessibility tests for key components in the console test suite

### Tested environments

The console and applications have been reviewed against WCAG 2.2 AA and
tested on **Linux** and **Android**. They are expected to work in current
versions of Chrome, Edge, Firefox and Safari, and with common assistive
technologies (NVDA, JAWS, VoiceOver, Orca, TalkBack).

### Known limitations

- Windows, macOS, iOS and HarmonyOS have not been tested with assistive
  technologies yet.
- The experimental HarmonyOS application has not been verified on a physical
  device.
- Some technical content — JSON editors, request/response captures and raw
  upstream responses — is shown as code and can be verbose with a screen reader.
- Accessibility is not yet checked automatically for every page in CI.

### Reporting a barrier

If something in GPROXY is hard or impossible to use, please tell us:

- Open an [accessibility issue](https://github.com/LeenHawk/gproxy/issues/new?template=accessibility.yml)
- Or email <leenhawk@leenhawk.com> if you prefer not to post publicly

Helpful details: the page or screen, the GPROXY version and distribution, your
operating system and browser, any assistive technology you use, what you
tried to do and what happened.

GPROXY is maintained by a single developer. We aim to acknowledge reports
within a week; barriers that block a core task (setup, sign-in, managing
providers and keys) are treated like high-priority bugs.

---

## 简体中文

### 承诺

GPROXY 希望每个人都能顺利部署和管理自己的网关，包括使用键盘、读屏软件、放大、
高对比度或大字体的用户。我们以 [WCAG 2.2](https://www.w3.org/TR/WCAG22/) AA
级为下列界面的目标，并把无障碍障碍当作 Bug 处理。

这是目标而非认证：GPROXY 尚未经过第三方审计。

### 范围

- 管理控制台和用户门户，包括在浏览器中访问，以及内嵌在桌面端和移动端应用中
- 设置向导、登录页和 OAuth 授权页
- 文档站 <https://gproxy.leenhawk.com/>

网关 API 本身没有用户界面。上游服务商的登录页面不在本项目控制范围内。

### 已经做到的

- 可完全用键盘操作；提供“跳到主要内容”链接；焦点框始终可见，在 Windows
  高对比度（强制颜色）模式下同样可见
- 对话框关闭后，焦点回到打开它的控件
- 控件都有标签，表单错误和说明与对应字段关联
- 每个页面有独立的页面标题，导航中标出当前页面
- 宽表格是可用键盘聚焦的滚动区域
- 图表附有等价的数据表
- 浅色和深色主题，支持“减少动态效果”，界面提供英文、简体中文、繁体中文和俄文
- 桌面端应用支持标准缩放快捷键；Android 应用跟随系统字体大小并支持双指缩放
- 控制台测试套件中包含关键组件的自动化无障碍测试

### 已测试的环境

控制台和应用已对照 WCAG 2.2 AA 进行审查，并在 **Linux** 和 **Android** 上测试。
预期可在最新版 Chrome、Edge、Firefox 和 Safari 中，以及常见辅助技术
（NVDA、JAWS、VoiceOver、Orca、TalkBack）下正常使用。

### 已知限制

- Windows、macOS、iOS 和鸿蒙尚未配合辅助技术进行测试。
- 实验性的鸿蒙应用尚未在真机上验证。
- 部分技术性内容——JSON 编辑器、请求/响应抓包和上游原始响应——以代码形式展示，
  读屏时可能较冗长。
- CI 尚未对所有页面进行自动化无障碍检查。

### 报告障碍

如果 GPROXY 中有难以使用或无法使用的地方，请告诉我们：

- 提交[无障碍问题](https://github.com/LeenHawk/gproxy/issues/new?template=accessibility.yml)
- 不便公开时，也可以发邮件到 <leenhawk@leenhawk.com>

建议附上：页面或界面、GPROXY 版本和发行形式、操作系统和浏览器、使用的辅助技术、
你想完成的操作以及实际发生的情况。

GPROXY 由一名开发者维护。我们争取在一周内回复报告；阻碍核心任务（部署、登录、
管理服务商和密钥）的障碍会按高优先级 Bug 处理。

---

## Русский

### Обязательства

GPROXY стремится к тому, чтобы каждый мог развернуть свой шлюз и управлять им,
в том числе люди, которые пользуются клавиатурой, программой чтения с экрана,
экранной лупой, режимом высокой контрастности или крупным шрифтом. Для
перечисленных ниже интерфейсов мы ориентируемся на уровень AA
[WCAG 2.2](https://www.w3.org/TR/WCAG22/) и считаем барьеры доступности
ошибками.

Это цель, а не сертификация: GPROXY не проходил независимый аудит.

### Область применения

- Консоль управления и пользовательский портал — в браузере, а также
  встроенные в настольные и мобильные приложения
- Мастер настройки, страницы входа и страницы согласия OAuth
- Сайт документации <https://gproxy.leenhawk.com/>

Сам API шлюза не имеет пользовательского интерфейса. Страницы входа вышестоящих
провайдеров находятся вне контроля проекта.

### Что уже сделано

- Полное управление с клавиатуры, ссылка «Перейти к основному содержимому» и
  постоянно видимая рамка фокуса, в том числе в режиме высокой контрастности
  Windows (принудительные цвета)
- После закрытия диалога фокус возвращается к элементу управления, который его
  открыл
- У элементов управления есть подписи, а ошибки и описания форм связаны с
  соответствующими полями
- У каждой страницы собственный заголовок, а текущая страница отмечена в
  навигации
- Широкие таблицы — это прокручиваемые области, доступные с клавиатуры
- К диаграммам прилагается эквивалентная таблица данных
- Светлая и тёмная темы, поддержка режима уменьшения движения, а также
  интерфейс на английском, упрощённом китайском, традиционном китайском и
  русском языках
- Настольные приложения поддерживают стандартные сочетания клавиш для
  масштабирования; приложение для Android учитывает системный размер шрифта и
  поддерживает масштабирование жестом
- Автоматические тесты доступности ключевых компонентов в наборе тестов консоли

### Протестированные среды

Консоль и приложения проверены на соответствие WCAG 2.2 AA и протестированы в
**Linux** и **Android**. Ожидается, что они работают в актуальных версиях
Chrome, Edge, Firefox и Safari, а также с распространёнными вспомогательными
технологиями (NVDA, JAWS, VoiceOver, Orca, TalkBack).

### Известные ограничения

- Windows, macOS, iOS и HarmonyOS ещё не тестировались со вспомогательными
  технологиями.
- Экспериментальное приложение для HarmonyOS не проверялось на физическом
  устройстве.
- Часть технического содержимого — редакторы JSON, захваты запросов и ответов,
  а также необработанные ответы вышестоящих провайдеров — отображается в виде
  кода и при чтении с экрана может быть многословной.
- Доступность пока проверяется в CI автоматически не для всех страниц.

### Сообщить о барьере

Если чем-то в GPROXY трудно или невозможно пользоваться, сообщите нам:

- Создайте [issue о доступности](https://github.com/LeenHawk/gproxy/issues/new?template=accessibility.yml)
- Или напишите на <leenhawk@leenhawk.com>, если не хотите писать публично

Полезные сведения: страница или экран, версия и вариант дистрибутива GPROXY,
ваши операционная система и браузер, используемые вспомогательные технологии,
что вы пытались сделать и что произошло.

GPROXY поддерживает один разработчик. Мы стараемся подтверждать получение
сообщений в течение недели; барьеры, мешающие выполнению основных задач
(развёртывание, вход, управление провайдерами и ключами), рассматриваются как
ошибки высокого приоритета.
