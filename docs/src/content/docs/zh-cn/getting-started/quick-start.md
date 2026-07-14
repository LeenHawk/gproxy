---
title: 快速开始
description: 启动 GPROXY v2，设置管理员账号，在 Console 配置路由并发出第一条请求。
---

本页会启动一个本地 native GPROXY v2 实例，并通过内嵌 Console 完成配置。不需要提前
准备或导入配置 Bundle。

## 1. 下载 GPROXY

从[下载页](/zh-cn/getting-started/downloads/)或
[最新 GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest)选择适合当前
平台的安装包。

:::caution[普通使用不要从源码构建]
如果只是使用 GPROXY 而不是参与开发，请不要克隆仓库，也不要安装 Cargo 和 pnpm。
Release 下载已经包含优化后的二进制与内嵌 Console。开发者可使用安装页中明确标注的
[源码构建说明](/zh-cn/getting-started/installation/#从源码构建仅开发者)。
:::

## 2. 设置管理员账号

安装 MSI、DMG 或 DEB 后打开 GPROXY。首次运行会要求输入管理员用户名和非空密码，
并询问是否在登录系统时自动启动 GPROXY。Launcher 不保存明文密码；GPROXY 会使用
Argon2id 生成密码哈希并持久化到 data store，供后续登录验证。

Android APK 的 launcher 也提供用户名、密码和自动启动开关。该开关同时控制打开 App
和设备开机后的自动启动；开启时请在 Android 系统提示中允许后台运行，避免电池优化停止
Service。

如果下载的是便携压缩包，在终端直接启动：

```bash
chmod +x ./gproxy
./gproxy --data-dir ./data \
  --admin-user admin \
  --admin-password change-me-please
```

对外暴露 GPROXY 前，请把示例密码换成强密码。每次传入 `--admin-password` 都会强制
upsert 对应管理员；后续启动时应移除这个参数，除非确实要重置密码。

## 3. 登录 Console

打开 <http://127.0.0.1:8787/console>，使用刚才设置的管理员用户名和密码登录。

如果需要加密保存 secret，应在存入 provider credential 前设置 `GPROXY_MASTER_KEY`，
值必须是标准 base64 编码的 32 字节。不设置时 GPROXY 使用明文 secret 模式并输出 warning。

## 4. 配置 Provider 和 Route

在 Console 中：

1. 创建 **Provider**，再添加真实的上游 API credential。
2. 创建 **Route**，并添加指向该 provider 与上游模型的 route member。
3. 创建 user API key，并授予该用户调用这条 route 的权限。

记下 route name 和生成的 user API key。Provider、credential、route、permission、quota
及其他日常设置都以 Console 中的持久化配置为准。

## 5. 发起 Gateway 请求

把下面两个占位符替换成刚才在 Console 创建的值：

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer <your-user-api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "<your-route-name>",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

聚合入口 `/v1` 会把模型名解析为 route 或 alias。Provider scoped 请求使用
`/{provider}/v1/...`，请求体中填写真实的上游模型 id。

继续阅读[第一条请求](/zh-cn/getting-started/first-request/)，了解 OpenAI、Claude、Gemini
请求格式以及两种路由模式。
