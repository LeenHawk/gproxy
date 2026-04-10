---
title: Troubleshooting
description: Common GPROXY errors, their causes, and how to fix them.
---

## 1) `401 unauthorized`

**Messages:**

- `missing API key`
- `invalid or disabled API key`
- `session expired or invalid`
- `user not found`
- `invalid username or password`
- `session token required (use /login to obtain one)`

**Causes:**

- Request has no auth header. Add `Authorization: Bearer <key>`, `x-api-key`, `x-goog-api-key`, or `?key=<value>`.
- API key does not exist in the database or belongs to a disabled user.
- Session token has expired (sessions are memory-only, 24h TTL by default, lost on restart).
- Using an API key on `/user/*` routes. These routes require a session token from `/login`.

**Fix:** Verify the key exists and is enabled in the admin console. For session routes, call `POST /login` first.

## 2) `403 forbidden`

**Messages:**

- `admin access required`
- `user is disabled`

**Causes:**

- Calling `/admin/*` with an API key owned by a non-admin user. Admin routes require a key owned by a user with `is_admin = true`.
- Calling `/admin/*` with a session token from a non-admin user.
- User account was disabled after the session was created. The session is still valid but the user check fails.

**Fix:** Use a key or session belonging to an admin user. Re-enable the user if it was accidentally disabled.

## 3) `503 all eligible credentials exhausted`

**Causes:**

- Provider has zero credentials configured.
- All credentials are marked as dead (persistent auth failures like 401/403 from upstream).
- All credentials are in cooldown (temporary, after upstream 429 or 5xx).
- The target model is restricted and no credential has access to it.

**Fix:**

1. Check credential statuses in admin console (`/admin/credential-statuses/query`).
2. If credentials are dead, verify the API keys are still valid upstream.
3. If credentials are in cooldown, wait for the cooldown period to expire or manually reset the status via `/admin/credential-statuses/update`.
4. Add more credentials to spread load.

## 4) `model must have provider prefix (provider/model) or match an alias`

**When:** Calling unscoped routes (`/v1/chat/completions`, `/v1/messages`, etc.) where the provider is not in the URL.

**Cause:** The `model` field in the request body does not include a provider prefix and does not match any configured model alias.

**Fix:** Either:

- Use `"model": "openai/gpt-4.1"` (provider prefix).
- Use a scoped route: `/{provider}/v1/chat/completions` where the model field can be just `"gpt-4.1"`.
- Configure a model alias in the admin console that maps an alias name to a provider + model.

## 5) `unsupported <channel> request route: (<operation>, <protocol>)`

**Example:** `unsupported openai request route: (Embedding, ClaudeMessages)`

**Cause:** The provider's dispatch table does not have a route for the requested (operation, protocol) combination. This happens when you send a request to a protocol endpoint that the target channel does not support.

**Fix:**

- Check which operations and protocols the provider supports. Use `/admin/providers/default-dispatch` to see the channel's dispatch table.
- Use the correct endpoint for the provider. For example, don't send Claude Messages protocol requests to an OpenAI-only provider.
- If the provider should support this combination, update the provider's dispatch configuration.

## 6) `no request transform for (<src_op>, <src_proto>) -> (<dst_op>, <dst_proto>)`

**Cause:** GPROXY received a request in one protocol (e.g. OpenAI Chat) and the dispatch table routes it to a different protocol (e.g. Gemini), but there is no cross-protocol transform implemented for this specific pair.

**Fix:**

- Use same-protocol passthrough when possible (send OpenAI requests to OpenAI-compatible channels).
- Check the provider's dispatch table to see which transforms are configured.
- Not all cross-protocol transforms exist. Some combinations are intentionally unsupported.

## 7) `Failed to deserialize the JSON body`

**Cause:** Request body is missing required fields, has wrong types, or is malformed JSON.

**Fix:**

- Check the request body matches the expected API schema for the endpoint.
- Common mistakes: missing `model` field, missing `messages` array, wrong `content` type.
- Validate JSON syntax (trailing commas, unquoted keys).
