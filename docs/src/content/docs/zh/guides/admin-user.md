---
title: 管理端与用户端
description: Admin 与 User 两类角色的职责边界、接口能力与示意图。
---

GPROXY 将管理能力分为两层：`admin`（平台运维）和 `user`（业务调用方）。

## Admin（平台管理员）

管理员侧负责平台级配置和治理，典型能力包括：

- 全局配置读写与导入导出
- Provider、Credential、CredentialStatus 管理
- 用户与用户密钥管理
- 请求审计与用量查询
- 系统自更新能力

示意图：

![Admin 架构示意图](/admin.jpg)

## User（业务用户）

用户侧只管理自己的资源，典型能力包括：

- 查询/新增/删除自己的 API Key
- 查询自己的 usage 明细与聚合
- 使用自己的 key 调用 Provider 代理接口

示意图：

![User 架构示意图](/user.jpg)

## 角色边界建议

- Admin key 仅用于运维与配置变更，不用于业务流量。
- 业务调用建议为每个团队/服务单独发放 user key，便于审计和隔离。
- 生产环境建议启用 `mask_sensitive_info = true`，避免敏感请求体落库。
