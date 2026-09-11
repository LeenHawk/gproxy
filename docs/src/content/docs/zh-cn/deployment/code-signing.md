---
title: Code signing policy（代码签名政策）
description: "GPROXY Windows 签名状态、PR 审查、隐私说明与签名验证。"
---

## 当前状态

GPROXY 已申请 SignPath Foundation，正在等待审核，尚未启用 Windows 正式签名。
接入代码和本政策不代表申请已经获批，也不代表历史发布文件已签名。

获批并启用后，Windows 标签发布使用：
**Free code signing provided by [SignPath.io](https://signpath.io), certificate
by [SignPath Foundation](https://signpath.org).**

## 签名范围与代码审查

仅签署 [LeenHawk/gproxy](https://github.com/LeenHawk/gproxy) 仓库发布工作流构建的产物，
覆盖 Windows x86_64 和 ARM64 的便携 ZIP 内 `gproxy.exe`、MSI 安装包及其内的 EXE、
PowerShell 和 VBScript 启动脚本。

代码提交者和 PR 审查者为 [LeenHawk](https://github.com/LeenHawk)。
发布代码通过 GitHub PR 审查后合入受保护的 `main`，从已审查的提交创建发布标签；
CI 使用自动签名策略提交签名请求、下载结果并验证产物。
签名参与者须在 GitHub 和 SignPath 开启多因素认证。

启用后，稳定版及预发布版标签构建须取得可信且带时间戳的签名才能发布；
失败、拒绝和超时均阻止发布。持续构建的 `staging` 版本不签名。
Windows x86_64 和 ARM64 发布二进制均在签名前使用 UPX 加壳。

## 隐私说明

GPROXY 是自托管网关。请求及附带内容会发送到运营者配置的上游供应商；
OAuth 授权和凭证刷新会连接相应供应商。数据库、用量记录、请求日志、保留期限和
访问权限由运营者管理，所使用的供应商和部署服务的隐私政策同样适用。

更新检查和下载默认连接 GitHub。打开 Console 可能触发网关从
`gproxy.leenhawk.com` 获取签名公告。这些连接会向目标及其托管服务暴露普通连接信息，
包括来源 IP 和 User-Agent；公告请求的 User-Agent 包含 GPROXY 版本。
公告和更新请求不附带推理请求正文或上游凭证。出站代理会改变目标看到的来源 IP。
本项目不承诺所有网络连接都必须由用户逐次手动触发。

Windows 首次设置会询问是否启用自启动。MSI 可通过 Windows 设置卸载，保留的用户
数据按需另行删除。便携版须先停止进程，再删除解压文件，并按需保留或删除数据目录。

## 验证下载

解压便携版后，在 PowerShell 中检查 EXE；检查 MSI 时替换为安装包路径：

```powershell
Get-AuthenticodeSignature .\gproxy.exe |
  Format-List Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

正式签名产物应显示 `Status: Valid`、SignPath Foundation 发布者证书及时间戳证书。
以实际文件为准，不能仅凭版本名或发布日期认定已经签名。
签名不能保证杀毒软件不误报，也不能保证 SmartScreen 不提示。

更新清单的 Ed25519 签名供 GPROXY 自身验证更新使用，与 Windows 发布者信任是两回事。
