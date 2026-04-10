---
title: Admin and User
description: Responsibilities, API scopes, and diagrams for Admin and User roles.
---

GPROXY separates management into two roles: `admin` (platform operations) and `user` (business caller).

## Admin (platform operator)

Admin is responsible for platform-level configuration and governance:

- Global settings read/write and import/export
- Provider, Credential, and CredentialStatus management
- User and user-key management
- Request audit and usage query
- System self-update

Diagram:

![Admin architecture diagram](/admin.jpg)

## User (business caller)

User only manages self-owned resources:

- Query/create/delete own API keys
- Query own usage details and summaries
- Call provider proxy APIs with own keys

Diagram:

![User architecture diagram](/user.jpg)

## Boundary recommendations

- Use admin key only for operations and configuration changes, not business traffic.
- Issue dedicated user keys per team/service for auditability and isolation.
- In production, enable `mask_sensitive_info = true` to avoid storing sensitive payloads.
