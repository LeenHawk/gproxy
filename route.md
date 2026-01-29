# Routes

All admin routes require one of:
- `x-admin-key: <admin_key>`
- `authorization: Bearer <admin_key>`

Timestamps are unix seconds.

## Admin

### GET /admin/health
```bash
curl -H "x-admin-key: pwd" http://127.0.0.1:8787/admin/health
```

### GET /admin/config
```bash
curl -H "x-admin-key: pwd" http://127.0.0.1:8787/admin/config
```

### PUT /admin/config
```bash
curl -X PUT -H "x-admin-key: pwd" -H "content-type: application/json" \
  http://127.0.0.1:8787/admin/config \
  -d '{
    "host":"127.0.0.1",
    "port":8787,
    "admin_key":"pwd",
    "dsn":"sqlite:///path/to/gproxy.db",
    "proxy":null
  }'
```
Notes:
- When `dsn` changes, the server connects to the new database, syncs schema, writes config there, reloads snapshot, and switches to the new connection immediately.
- Response includes `dsn_changed: true|false`.
- `bind_changed` reports `host/port` changes and triggers an immediate rebind.
- `proxy_changed` reports `proxy` updates and takes effect immediately for new requests.

### GET /admin/providers
### POST /admin/providers
```bash
curl -X POST -H "x-admin-key: pwd" -H "content-type: application/json" \
  http://127.0.0.1:8787/admin/providers \
  -d '{
    "id": 1,
    "name": "openai",
    "config_json": {},
    "enabled": true
  }'
```

### PUT /admin/providers/{id}
### DELETE /admin/providers/{id}

### GET /admin/credentials
### POST /admin/credentials
```bash
curl -X POST -H "x-admin-key: pwd" -H "content-type: application/json" \
  http://127.0.0.1:8787/admin/credentials \
  -d '{
    "id": 1,
    "provider_id": 1,
    "name": "key-1",
    "secret": {"api_key":"..."},
    "meta_json": {},
    "weight": 1,
    "enabled": true
  }'
```
Or by name:
```bash
curl -X POST -H "x-admin-key: pwd" -H "content-type: application/json" \
  http://127.0.0.1:8787/admin/credentials \
  -d '{
    "provider_name": "claude",
    "name": "key-1",
    "secret": {"api_key":"..."},
    "meta_json": {},
    "weight": 1,
    "enabled": true
  }'
```
Notes:
- Claude credential: `secret.api_key` (or `secret` as a string), `meta_json.base_url` optional.

### PUT /admin/credentials/{id}
### DELETE /admin/credentials/{id}

### GET /admin/disallow
### POST /admin/disallow
```bash
curl -X POST -H "x-admin-key: pwd" -H "content-type: application/json" \
  http://127.0.0.1:8787/admin/disallow \
  -d '{
    "credential_id": 1,
    "scope_kind": "model",
    "scope_value": "gpt-4.1",
    "level": "cooldown",
    "until_at": 1730000000,
    "reason": "rate_limit"
  }'
```

### DELETE /admin/disallow/{id}

### GET /admin/users
### POST /admin/users
```bash
curl -X POST -H "x-admin-key: pwd" -H "content-type: application/json" \
  http://127.0.0.1:8787/admin/users \
  -d '{
    "id": 2,
    "name": "alice"
  }'
```

### DELETE /admin/users/{id}

### GET /admin/keys
### POST /admin/keys
```bash
curl -X POST -H "x-admin-key: pwd" -H "content-type: application/json" \
  http://127.0.0.1:8787/admin/keys \
  -d '{
    "id": 2,
    "user_id": 2,
    "key_value": "user-key-1",
    "label": "default",
    "enabled": true
  }'
```

### DELETE /admin/keys/{id}
### PUT /admin/keys/{id}/disable

### POST /admin/reload
```bash
curl -X POST -H "x-admin-key: pwd" http://127.0.0.1:8787/admin/reload
```

### GET /admin/stats
```bash
curl -H "x-admin-key: pwd" http://127.0.0.1:8787/admin/stats
```

## Proxy

### /{provider}/{*path}
Proxy all provider requests.
```bash
curl -H "x-api-key: <user_key>" http://127.0.0.1:8787/openai/v1/models
```
