---
title: 内嵌控制台
description: gproxy 二进制内自带的 React 控制台。
---

gproxy 把一整个 **React 控制台**打包进了发布二进制。服务启动后，控制台
就挂载在 `/console` —— 不需要单独跑或部署前端进程。

## 访问控制台

1. 启动 gproxy (参见 [快速开始](/zh-cn/getting-started/quick-start/))。
2. 浏览器访问 `http://<host>:<port>/console`。
3. 使用任一启用用户的用户名/密码登录。若该用户 `is_admin = true`，
   你会看到管理员视图。

登录走 `POST /login`，返回一个**会话 token**，由 UI 保存，后续请求
以 `Authorization: Bearer <session_token>` 携带。

## 控制台能做什么

- **供应商** —— 创建、编辑、禁用；通过通道感知的结构化编辑器修改 settings 和
  credentials；查看每个凭证的健康状态。
- **模型** —— 浏览某个供应商下的模型列表，编辑价格、启停、定义别名，
  或使用 **Pull Models** 从上游拉取真实的模型列表。
- **用户** —— 创建用户，签发/吊销 API key，重置密码，切换 admin 身份。
- **权限 / 限流 / 配额** —— 针对三种访问控制机制 (见
  [权限、限流与配额](/zh-cn/guides/permissions/)) 提供的按用户编辑器。
- **可观测性** —— 用量仪表盘、上下游请求日志 (若开启)、健康历史。
- **设置** —— 全局代理设置、日志开关、升级源、TOML 重写规则与脱敏规则编辑器。

## 重新构建控制台

控制台源码在 `frontend/console/` 下。修改之后：

```bash
cd frontend/console
pnpm install
pnpm build
```

`pnpm build` 会把产物写入 server crate 通过 `include_dir!` 嵌入的目录。
前端构建完成后再构建二进制：

```bash
cargo build -p gproxy --release
```

运行时无需挂载任何静态文件目录 —— 资源全部嵌入在可执行文件里。

## 以开发模式运行控制台

若需要高效迭代前端，可以让 Vite dev server 指向一个运行中的后端：

```bash
cd frontend/console
pnpm install
pnpm dev
```

设置 Vite dev 代理把 `/admin/*`、`/v1/*`、`/login` 转发到本地 `gproxy`
实例即可。

## 放到反向代理后

控制台的鉴权方式是用户名/密码 + bearer token —— 它本身不对接外部 SSO。
若需要 SSO，请把 gproxy 放在反向代理后面，在代理层做鉴权，仅对认证后的
会话放行 `/console` 和 `/admin/*`；LLM 路由继续走 API-key 流程。
