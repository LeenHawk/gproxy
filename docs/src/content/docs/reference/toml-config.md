---
title: TOML Config
description: Compatibility note for the v1 TOML seed file and the supported v2 JSON import/export format.
---

gproxy v2 does not read a `gproxy.toml` seed file on the native serve path. The
v1 `GPROXY_CONFIG` / TOML bootstrap model has been replaced by:

- startup CLI flags and environment variables for process-level settings;
- persisted control-plane rows for live configuration;
- JSON bundle import/export for reproducible bootstrap and migration workflows.

This page keeps the `toml-config` slug for continuity with v1 documentation,
but the supported v2 format is JSON, not TOML.

## Supported import/export commands

Import a bundle into the configured persistence backend and exit:

```bash
./gproxy \
  --persistence db \
  --dsn 'sqlite:///var/lib/gproxy/gproxy.db?mode=rwc' \
  import --in ./import.json
```

Export control-plane configuration, including plaintext secrets, and exit:

```bash
./gproxy \
  --persistence db \
  --dsn 'sqlite:///var/lib/gproxy/gproxy.db?mode=rwc' \
  export --out ./export.json
```

The export command writes with owner-only permissions on Unix and uses an
atomic same-directory rename, but the file still contains plaintext provider
credentials and user API keys. Treat it as a secret.

## First-boot import hook

On the normal serve path, `GPROXY_IMPORT_FILE` imports a JSON bundle only when
the store is empty:

```bash
GPROXY_IMPORT_FILE=/etc/gproxy/import.json ./gproxy
```

The hook runs before admin bootstrap. A bundle-provided admin user prevents
random first-boot admin creation. Once the store has any provider or user rows,
the hook is skipped.

## Bundle shape

A v2 bundle has `schema_version: 1` and arrays of persistence input records.
References are raw numeric ids, so a bundle that cross-references records must
pin explicit ids.

```json
{
  "schema_version": 1,
  "orgs": [
    { "id": 1, "name": "default", "enabled": true, "description": null }
  ],
  "users": [
    {
      "id": 1,
      "name": "admin",
      "org_id": 1,
      "team_id": null,
      "password": "$argon2id$...",
      "enabled": true,
      "is_admin": true
    }
  ],
  "user_keys": [
    {
      "id": 1,
      "user_id": 1,
      "api_key": "sk-replace-with-a-long-random-key",
      "label": "bootstrap",
      "enabled": true
    }
  ],
  "providers": [
    {
      "id": 1,
      "name": "openai-main",
      "channel": "openai",
      "label": null,
      "settings_json": { "base_url": "https://api.openai.com" },
      "credential_strategy": "round_robin",
      "proxy_url": null,
      "tls_fingerprint": null,
      "enabled": true
    }
  ],
  "credentials": [
    {
      "id": 1,
      "provider_id": 1,
      "label": "primary",
      "kind": "api_key",
      "secret_json": { "api_key": "sk-provider-key" },
      "weight": 100,
      "rpm_limit": null,
      "tpm_limit": null,
      "proxy_url": null,
      "tls_fingerprint": null,
      "enabled": true
    }
  ],
  "provider_models": [
    {
      "id": 1,
      "provider_id": 1,
      "model_id": "gpt-4.1-mini",
      "display_name": "GPT-4.1 mini",
      "pricing_json": { "input": "0.40", "output": "1.60" },
      "variants_json": null,
      "enabled": true
    }
  ],
  "routes": [
    {
      "id": 1,
      "name": "main",
      "strategy": "failover",
      "enabled": true,
      "description": null,
      "settings_json": null
    }
  ],
  "route_members": [
    {
      "id": 1,
      "route_id": 1,
      "provider_id": 1,
      "upstream_model_id": "gpt-4.1-mini",
      "weight": 100,
      "tier": 0,
      "enabled": true
    }
  ],
  "aliases": [
    { "id": 1, "alias": "default-chat", "route_id": 1 }
  ]
}
```

## Supported top-level arrays

| Array | Purpose |
| --- | --- |
| `orgs`, `teams`, `users`, `user_keys` | Identity, admin login, and API-key material. Imported API keys are digested for lookup and sealed for storage. |
| `route_permissions`, `rate_limits`, `quotas` | Org/team/user-scoped access control, token limits, and spend quotas. |
| `providers`, `credentials`, `provider_models` | Upstream providers, sealed credentials, exposed upstream models, optional pricing and variants. |
| `routes`, `route_members`, `aliases` | Logical model names, backend pools, and aliases. |
| `routing_rules` | Per-provider transform dispatch rows. Provider creation through the admin API seeds defaults automatically; raw bundle imports only import rows you provide. |
| `rule_sets`, `rules`, `provider_rule_sets` | Reusable request/response mutation rule sets and provider attachments. |
| `instance_settings` | Singleton instance behavior such as retention and tokenizer download settings. |

## Live configuration source of truth

After import, the persistence backend is the source of truth. Editing a JSON
file on disk does not change a running server unless you run the import command
or start with `GPROXY_IMPORT_FILE` against an empty store. For normal operations,
use the console or admin API.

## What changed from v1

| v1 concept | v2 replacement |
| --- | --- |
| `GPROXY_CONFIG=gproxy.toml` | No current v2 equivalent. Use environment variables for process settings and JSON import/export for seeded control-plane data. |
| TOML provider/model/user arrays | JSON bundle arrays matching v2 persistence input records. |
| Re-reading TOML after edits | Unsupported. Live rows are edited through the admin API/console and reflected through snapshot invalidation. |
| `DATABASE_SECRET_KEY` runtime secret encryption | `GPROXY_MASTER_KEY` for v2 sealed secrets; `DATABASE_SECRET_KEY` is only for reading encrypted v1 data during migration. |
