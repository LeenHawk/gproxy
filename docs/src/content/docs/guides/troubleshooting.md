---
title: Troubleshooting
description: Common GPROXY errors and investigation path.
---

## 1) `401 unauthorized`

Check:

- Request includes `x-api-key` (or a compatible auth header)
- Key exists
- User bound to the key is `enabled`

## 2) `403 forbidden` (admin routes)

Usually means current key is not owned by admin user (`id=0`).

## 3) `503 all eligible credentials exhausted`

Common causes:

- Channel has no available credentials
- Credential has been marked as `dead`
- Target model is in `partial` cooldown
- Upstream keeps returning 429/5xx

## 4) `model must be prefixed as <provider>/...`

When calling unscoped routes (for example `/v1/chat/completions`), `model` must use `<provider>/<model>` format.

## 5) Realtime WebSocket unavailable

`/v1/realtime` is currently not implemented; use `/v1/responses` (HTTP) instead.
