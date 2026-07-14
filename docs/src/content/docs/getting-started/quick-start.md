---
title: Quick Start
description: Start GPROXY v2, create an administrator login, configure routing in Console, and send a request.
---

This guide starts a local native GPROXY v2 instance and configures it through
the embedded Console. You do not need to prepare or import a configuration
bundle.

## 1. Download GPROXY

Choose the package for your platform on the
[Downloads](/getting-started/downloads/) page or the
[latest GitHub Release](https://github.com/LeenHawk/gproxy/releases/latest).

:::caution[Do not build from source for normal use]
If you want to use GPROXY rather than develop it, do not clone the repository
or install Cargo and pnpm. Release downloads already contain an optimized
binary and the embedded Console. Developers can use the clearly marked
[source-build instructions](/getting-started/installation/#build-from-source-developers-only).
:::

## 2. Set The Administrator Login

Open the installed GPROXY launcher from an MSI, DMG, or DEB package. On its
first run, enter an administrator username and a non-blank password. The setup
also asks whether GPROXY should start automatically when you sign in. The
password is passed only to this first start and is not saved by the launcher.

The Android APK provides the same username and password fields in its launcher.
Its automatic-start switch controls app-launch and device-boot startup. When
automatic startup is enabled, allow background operation in Android's system
prompt so battery optimization does not stop the service.

For a portable archive, start the binary directly:

```bash
chmod +x ./gproxy
./gproxy --data-dir ./data \
  --admin-user admin \
  --admin-password change-me-please
```

Choose a strong password instead of the example before exposing GPROXY. Supplying
`--admin-password` force-upserts that named administrator on every startup, so
omit the password option on later starts unless you intend to reset it.

## 3. Sign In To Console

Open <http://127.0.0.1:8787/console> and sign in with the administrator username
and password you just chose.

For encrypted at-rest secrets, set `GPROXY_MASTER_KEY` to standard base64 for
exactly 32 bytes before storing provider credentials. Without it, GPROXY uses
plaintext secret mode and logs a warning.

## 4. Configure A Provider And Route

In Console:

1. Create a **Provider**, then add its upstream API credential.
2. Create a **Route** and add a route member that targets the provider and
   upstream model.
3. Create a user API key and grant that user permission to call the route.

Keep the route name and generated user API key for the next step. Console is
the source of truth for providers, credentials, routes, permissions, quotas,
and other day-to-day settings.

## 5. Make A Gateway Request

Replace both placeholders with the values created in Console:

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

The aggregated `/v1` endpoint resolves the model as a route or alias. For a
provider-scoped request, use `/{provider}/v1/...` and send the upstream model
id instead.

Continue with [First Request](/getting-started/first-request/) for OpenAI,
Claude, and Gemini request shapes and the two routing modes.
