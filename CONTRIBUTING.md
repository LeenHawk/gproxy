# Contributing to GPROXY / 贡献指南

[English](#english) · [简体中文](#简体中文)

---

## English

Thank you for your interest in GPROXY. Bug reports, fixes, new channels,
translations and documentation improvements are all welcome. By participating
you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

### Before you start

- **Security vulnerabilities** must not be reported publicly; see
  [SECURITY.md](SECURITY.md).
- **Bugs and feature requests** go to
  [Issues](https://github.com/LeenHawk/gproxy/issues). Search existing issues
  first. For anything you prefer not to post publicly, email
  <leenhawk@leenhawk.com>.
- **Larger changes** — a new channel, a new protocol, a schema change or a new
  page in the console — should start with an issue so the design can be agreed
  before you write a lot of code.
- New upstream channels follow
  [Adding a channel](https://gproxy.leenhawk.com/guides/adding-a-channel/).

### Branches

| Branch | Purpose | Target your PR here when… |
| --- | --- | --- |
| `main` | Released functionality and bug fixes; publishes to the beta channel | you fix a bug in the current stable release |
| `dev` | New features and the next major version; publishes to the nightly channel | you add a feature or change behavior |

`dev` is regularly rebased onto `main`. Keep your branch up to date by
**rebasing** rather than merging, and keep each pull request focused on one
change.

### Development setup

Native builds require stable Rust, Go and Clang. The console and docs require
Node.js 22.12+ (24 LTS recommended) and pnpm. Linux desktop builds also require
the WebKitGTK 4.1, GTK 3 and libsoup 3 development packages.

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build      # also syncs the embedded console assets
cargo run -p gproxy -- serve
```

Use a fresh data directory for local testing, and never point a development
instance at a production database or reuse production upstream credentials.

### Checks

Run the checks relevant to your change before opening a pull request; CI runs
all of them:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm --dir console lint       # includes TypeScript, ESLint and i18n parity
pnpm --dir console test
pnpm --dir docs check && pnpm --dir docs build
```

See [Building from source](https://gproxy.leenhawk.com/deployment/release-build/)
for desktop, mobile and WASM builds.

### Guidelines

- **Code style** — follow the surrounding code: naming, comment density and
  idioms. Keep changes minimal and avoid unrelated reformatting.
- **Tests** — bug fixes should come with a regression test where practical.
  Do not commit scratch or one-off reproduction tests.
- **Translations** — console strings live in `console/src/locales/`
  (`en`, `zh-CN`, `zh-TW`); add a key to all three. `pnpm --dir console lint`
  checks parity.
- **Documentation** — user-visible changes should update the docs in
  `docs/src/content/docs/`, in both English and `zh-cn/`.
- **Generated types** — if you change DTOs exported to TypeScript, run
  `pnpm --dir console types` and commit the result.
- **Secrets and captures** — never commit API keys, OAuth tokens, cookies,
  account identifiers or unredacted upstream responses, including in tests,
  fixtures, screenshots and issue comments.

### Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<optional scope>): <summary in imperative mood>
```

Common types: `feat`, `fix`, `docs`, `ci`, `build`, `refactor`, `perf`,
`test`, `chore`. Examples:

```text
fix(claude): keep the signature on empty thinking blocks
feat(console): add a credential health filter
```

### Pull requests

1. Fork the repository and create a branch from `main` (bug fix) or `dev`
   (feature).
2. Make your change, with tests and documentation.
3. Run the checks above.
4. Open a pull request against the matching branch and fill in the template.

The maintainer may ask for changes, squash commits, or rebase your branch when
merging.

### License

GPROXY is licensed under **AGPL-3.0-or-later**; some crates
(`gproxy-protocol`, `gproxy-protocol-macros`, `gproxy-client`, `gproxy-cache`,
`gproxy-file`, `gproxy-seaorm`, `gproxy-tokenizer`) are **MIT**. By submitting
a contribution, you agree that it is licensed under the license of the files
or crate you change.

---

## 简体中文

感谢你关注 GPROXY。欢迎提交问题反馈、修复、新渠道、翻译和文档改进。
参与本项目即表示你同意遵守[行为准则](CODE_OF_CONDUCT.md)。

### 开始之前

- **安全漏洞**请勿公开报告，详见 [SECURITY.md](SECURITY.md)。
- **Bug 与功能建议**请提交到 [Issues](https://github.com/LeenHawk/gproxy/issues)，
  提交前请先搜索是否已有相同问题。不便公开的内容可以发邮件到
  <leenhawk@leenhawk.com>。
- **较大的改动**——新渠道、新协议、数据库结构变更或控制台新页面——请先开 Issue
  讨论方案，再动手写大量代码。
- 新增上游渠道请参考
  [添加渠道](https://gproxy.leenhawk.com/zh-cn/guides/adding-a-channel/)。

### 分支

| 分支 | 用途 | 何时向它提 PR |
| --- | --- | --- |
| `main` | 已发布功能与 Bug 修复；发布到 beta 通道 | 修复当前稳定版中的 Bug |
| `dev` | 新功能和下一个大版本；发布到 nightly 通道 | 新增功能或改变现有行为 |

`dev` 会定期 rebase 到 `main` 上。请用 **rebase** 而不是 merge 来同步你的分支，
每个 Pull Request 只做一件事。

### 开发环境

原生构建需要 stable Rust、Go 和 Clang。控制台和文档需要 Node.js 22.12+
（推荐 24 LTS）和 pnpm。Linux 桌面端构建还需要 WebKitGTK 4.1、GTK 3 和
libsoup 3 的开发包。

```sh
pnpm --dir console install --frozen-lockfile
pnpm --dir console build      # 同时同步内嵌的控制台资源
cargo run -p gproxy -- serve
```

本地测试请使用全新的数据目录，切勿让开发实例连接生产数据库或复用生产环境的上游凭据。

### 检查

提交 Pull Request 前请运行与你的改动相关的检查，CI 会全部运行：

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm --dir console lint       # 包括 TypeScript、ESLint 和多语言一致性检查
pnpm --dir console test
pnpm --dir docs check && pnpm --dir docs build
```

桌面端、移动端和 WASM 构建见
[从源码构建](https://gproxy.leenhawk.com/zh-cn/deployment/release-build/)。

### 约定

- **代码风格**——与周边代码保持一致：命名、注释密度和惯用写法。改动尽量精简，
  不要夹带无关的格式调整。
- **测试**——修复 Bug 时尽量附上回归测试。不要提交临时的复现测试。
- **翻译**——控制台文案位于 `console/src/locales/`（`en`、`zh-CN`、`zh-TW`），
  新增键需在三种语言中都添加；`pnpm --dir console lint` 会检查一致性。
- **文档**——用户可见的改动请同步更新 `docs/src/content/docs/` 中的文档，
  英文和 `zh-cn/` 都要改。
- **生成类型**——修改了导出到 TypeScript 的 DTO 时，请运行
  `pnpm --dir console types` 并提交生成结果。
- **密钥与抓包**——切勿提交 API Key、OAuth Token、Cookie、账号标识或未脱敏的
  上游响应，测试、fixture、截图和 Issue 评论中同样如此。

### 提交信息

使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 格式：

```text
<type>(<可选 scope>): <祈使语气的英文摘要>
```

常用类型：`feat`、`fix`、`docs`、`ci`、`build`、`refactor`、`perf`、
`test`、`chore`。示例：

```text
fix(claude): keep the signature on empty thinking blocks
feat(console): add a credential health filter
```

### Pull Request

1. Fork 本仓库，从 `main`（Bug 修复）或 `dev`（新功能）创建分支。
2. 完成改动，并附上测试和文档。
3. 运行上述检查。
4. 向对应分支提交 Pull Request，并填写模板。

维护者可能会要求修改，或在合并时压缩提交、rebase 你的分支。

### 许可证

GPROXY 采用 **AGPL-3.0-or-later** 许可；部分 crate（`gproxy-protocol`、
`gproxy-protocol-macros`、`gproxy-client`、`gproxy-cache`、`gproxy-file`、
`gproxy-seaorm`、`gproxy-tokenizer`）采用 **MIT** 许可。提交贡献即表示你同意
该贡献以你所修改的文件或 crate 的许可证授权。
