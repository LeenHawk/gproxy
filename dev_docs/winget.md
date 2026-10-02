# WinGet 自动提交

稳定版 Release 的 `publish` 任务成功后，调用 `winget-publish.yml`，分别为
`LeenHawk.GPROXY.Desktop` 和 `LeenHawk.GPROXY.CLI` 提交到
`microsoft/winget-pkgs`。main 的 staging、dev 的 nightly 和预发布版本不触发。

流程只提交 Microsoft 签名的 Windows x64/ARM64 MSIX，校验 SHA256SUMS、
Windows Authenticode 信任及 Microsoft 签名者、包标识/版本/架构、WinGet Schema。
未签名的版本暂不提交，等待每日 Store 同步。不同版本各自保留清单。
已存在的 MSIX 版本或同版本打开的 PR 会跳过；旧 ZIP 版本合并后可提交同版本
MSIX 替换清单。打开的旧 ZIP PR 会先等待合并，避免冲突提交。
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
产品描述或依赖变化时，需要同步更新模板/脚本。生成时移除 ZIP 的嵌套安装字段，
设置 MSIX 安装类型、升级行为及从包内派生的 PackageFamilyName/最低系统版本。

本地验证（只读下载，不提交；签名信任检查需 Windows 和 pwsh）：

```bash
python3 -m venv target/winget-venv
target/winget-venv/bin/pip install PyYAML==6.0.3 jsonschema==4.26.0
target/winget-venv/bin/python scripts/prepare-winget.py --version 4.0.0 --edition CLI
target/winget-venv/bin/python scripts/prepare-winget.py --version 4.0.0 --edition Desktop
```

清单位于 `dist/winget/manifests`。本地 Schema/哈希验证不能代替 Windows
实际安装测试；上游 PR 的安装验证状态需要单独检查。

## 每日 Store 同步

`store-sync.yml` 在签名 MSIX 和 SHA256SUMS 上传成功后，输出可提交的版本/产品。
一个产品的 x64、ARM64 都验证成功才进入 WinGet 提交；两个产品分别处理。
同步发现附件已签名也会继续检查 WinGet，因此前次 PR 提交失败可在下次重试。
使用同一个 `prepare-winget.py` 生成 MSIX 清单和 `WINGET_TOKEN` 提交 PR。
Store 同步的 dry run 不上传附件、不生成或提交 WinGet PR。
