---
title: "用户与 API Key"
description: "创建用户、分配管理范围，签发和轮换网关 API Key。"
---


控制台使用用户名和密码登录，客户端使用网关 API Key 调用模型。上游凭证在供应商中配置，不能用来登录 GPROXY。

## 首个管理员

Application 在首次设置向导中创建管理员。CLI 在空实例启动时创建管理员并显示生成的密码和 API Key，也可以通过以下变量预先指定：

```sh
export GPROXY_ADMIN_USER='admin'
export GPROXY_ADMIN_PASSWORD='your-initial-password'
export GPROXY_BOOTSTRAP_ADMIN_API_KEY='your-initial-gateway-key'
./gproxy serve
```

密码和密钥变量均可省略，由程序生成。**这些选项只用于首次初始化，不会重置已有用户的密码。** 不再支持通过 `GPROXY_BOOTSTRAP_CHANNELS` 自动创建供应商，请在控制台添加。

CLI / 容器的登录入口是 `/console/`。个人页和管理页共用登录会话，HTTP 会话 Cookie 名为 `gproxy_session`，生命周期由 `session_ttl_secs` 配置。Application 使用应用内控制台。

## 用户、组织与团队

实例管理员在访问控制中创建用户。用户角色为 `admin` 或 `user`；未设置密码的用户不能登录控制台，但可以使用获准签发的 API Key。

组织与团队通过成员关系分配用户和管理角色。API Key 可以绑定组织或团队，这个绑定用于确定凭证可见范围和费用预算。不能通过在请求中伪造组织、团队请求头来改变绑定。

组织和团队管理员只能管理其范围内开放的功能，不等于实例管理员。具体入口见[控制台与范围管理](/zh-cn/guides/console/)。

## 创建密钥

个人密钥在 **我的账户 → 密钥** 管理；实例管理员也可以在 **访问控制 → 用户密钥** 操作。

| 设置 | 含义 |
| --- | --- |
| 名称 | 方便区分不同应用或设备 |
| 归属 | 关联用户，以及可选的组织或团队 |
| 有效期与启用状态 | 过期或停用后拒绝新请求 |
| 管理权限 | 允许密钥访问管理操作，默认关闭；仍受所属用户和范围权限约束 |
| 保留密钥 | 允许以后再次查看完整值，默认不保留 |
| 费用预算 | 管理员可配置金额、周期和模型范围 |

创建后立即复制完整密钥。没有保留密钥原文、或仅从旧实例导入摘要的密钥，不能再次显示。显示操作仍需相应权限。

预算与新密钥可以一起保存，预算校验失败不会单独创建密钥。

## 轮换与撤销

密钥支持轮换，轮换后旧值失效，需要更新客户端。如果希望逐个迁移客户端而不中断旧配置，可以先创建第二把密钥，替换完成后再停用或删除旧密钥。

管理 API 的对应操作为：

- `POST /admin/api/api-keys/{id}/rotate`：轮换。
- `GET /admin/api/api-keys/{id}/secret`：查看已保留的密钥。
- 个人 API 使用 `/portal/api/keys/{id}/rotate` 和 `/portal/api/keys/{id}/secret`。

## 发送密钥

```text
Authorization: Bearer <gateway-key>
x-api-key: <gateway-key>
x-goog-api-key: <gateway-key>
```

选择客户端协议惯用的请求头即可，不要同时填写不同的密钥。管理 API 需要密钥启用管理权限；普通推理密钥不会因为所属用户是管理员就自动获得管理权限。

## OAuth 会话

GPROXY 可作为 OAuth 签发端，让客户端经用户授权取得访问令牌。OAuth 内部密钥与普通用户密钥不同，不能把它当作普通 Bearer Key 创建或显示。授权会话可以在控制台撤销。

客户端配置和授权流程见[CLI 客户端](/zh-cn/guides/cli-clients/)。
