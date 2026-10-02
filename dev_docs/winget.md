# WinGet 自动提交

稳定版 Release 的 `publish` 任务成功后，调用 `winget-publish.yml`，分别为
`LeenHawk.GPROXY.Desktop` 和 `LeenHawk.GPROXY.CLI` 提交到
`microsoft/winget-pkgs`。main 的 staging、dev 的 nightly 和预发布版本不触发。

流程下载指定版本的 Windows x64/ARM64 ZIP 和 SHA256SUMS，校验 SHA-256、
ZIP 内程序路径及微软 WinGet JSON Schema，然后使用官方 WinGet Create
提交 PR。已存在于上游的版本或已有打开的 PR 会跳过，不覆盖旧清单。
提交成功不代表上架；仍需微软验证、合并并同步到 WinGet 源。

也可手动运行 **Submit WinGet packages**，输入不带 `v` 的版本号。
`dry_run` 默认开启，只生成、校验并上传清单 artifact，不提交 PR；关闭后提交。
手动 dry run 会校验已经提交过的版本，便于检查流程。

## 凭证

仓库 Actions Secret：`WINGET_TOKEN`。使用 GitHub PAT classic，权限仅需
`public_repo`；创建时可选择 `No expiration`。不支持 fine-grained token。
官方说明：https://github.com/microsoft/winget-create/blob/main/doc/token.md

本机原文保存在 `dev_docs/winget-token.txt`，权限 0600，已加入本地
`.git/info/exclude`。这个文件不能提交到 Git。Secret 通过 stdin 写入：

```bash
gh secret set WINGET_TOKEN --repo LeenHawk/gproxy < dev_docs/winget-token.txt
```

CI 仅在提交步骤通过 `WINGET_CREATE_GITHUB_TOKEN` 环境变量提供凭证，
不使用命令行 token 参数。下载和重复检查使用只读 GITHUB_TOKEN。

## 维护

`.github/winget/manifests/.../4.0.0/` 是初始元数据模板。脚本将版本号、
发布日期、发行说明链接、下载链接和哈希替换为目标版本；其他包信息沿用模板。
产品描述、依赖或 ZIP 内文件名变化时，需要同步更新模板/脚本。

本地验证（只读下载，不提交）：

```bash
python3 -m venv target/winget-venv
target/winget-venv/bin/pip install PyYAML==6.0.3 jsonschema==4.26.0
target/winget-venv/bin/python scripts/prepare-winget.py --version 4.0.0 --edition CLI
target/winget-venv/bin/python scripts/prepare-winget.py --version 4.0.0 --edition Desktop
```

清单位于 `dist/winget/manifests`。本地 Schema/哈希验证不能代替 Windows
实际安装测试；上游 PR 的安装验证状态需要单独检查。
