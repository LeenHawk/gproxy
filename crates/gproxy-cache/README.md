# gproxy-cache

English | [简体中文](README.zh-CN.md)

Independent TTL state, atomic coordination and invalidation notifications. No
Store entities, routing policies, database connections, or implicit memory fallback.

| Feature | Backend |
|---|---|
| `memory` (default) | Process-local shared state; native and WASM |
| `redis` | Native async Redis/Valkey with TCP/TLS and independent subscribers |
| No features | Public contracts only; implement `Cache` for another backend |

```rust
use gproxy_cache::{Cache, MemoryCache, Replacement, CasOutcome};
use std::time::Duration;

# async fn example() -> gproxy_cache::Result<()> {
let cache = MemoryCache::default();
let ttl = Duration::from_secs(3600);
let version = cache.put("route/session", b"member-a".to_vec(), ttl).await?;
let result = cache.compare_exchange(
    "route/session", Some(version),
    Some(Replacement { value: b"member-b".to_vec(), ttl }),
).await?;
assert!(matches!(result, CasOutcome::Applied(Some(_))));
# Ok(())
# }
```

With `redis`, construct `RedisCache::connect(url, RedisOptions::new("deployment"))`.
Clones share a multiplexed command connection; independently constructed clients
with the same Redis database and namespace share state. Data and Pub/Sub channels
include an encoded namespace **and database number** (Redis Pub/Sub itself ignores
SELECT databases). Keys and topics are encoded without delimiter ambiguity. The
adapter targets a single Redis endpoint; it does not implement Sentinel discovery
or cluster redirection. Redis requires permissions for scripts, hashes, sorted
sets, expiry, TIME, and Pub/Sub. Real integration validation uses Redis 8; Valkey
and TLS paths have not been exercised against live servers in this change.

## Operations

| Operation | Semantics |
|---|---|
| `get / put / delete` | Opaque bytes, mandatory positive TTL, fresh version on every put |
| `compare_exchange` | Expected version or absence; replace with fresh version/TTL, or delete |
| `counter / increment` | Exact nonnegative integers through `i64::MAX`; atomic ceiling check and increment |
| `decrement` | Match counter generation before subtracting; underflow fails without mutation |
| `acquire_permit` | At most the requested number of live holders; independent TTL per holder |
| `acquire_lease` | Same permit domain, with limit 1; suitable for refresh coordination |
| `renew_permit / release_permit` | Only the matching, still-live owner can act |
| `publish / subscribe` | Application-defined byte payloads; lossy invalidation hints |

KV, counters, permits and topics have separate domains, so the same logical key
may exist in each. Operations are atomic per key, not across keys. Empty values
are valid; empty/oversized keys or values and invalid TTLs return errors. TTLs
round up to milliseconds, and use a shared representability bound (about 142,000
years). Memory uses a monotonic clock; Redis expiration/permits use server time.

Counters have a **fixed window**: only creation establishes TTL. Later increments
and decrements preserve it, including at zero. A newly created window has a fresh
generation so late decrements cannot touch it. Decrement is not an idempotent
receipt: a caller must release a charge exactly once and reconcile ambiguous
responses. Financial settlement and its durable idempotency ledger belong in Store.
Redis Lua compares decimal strings and obtains final values with HGET, so values
above `2^53` do not pass through floating-point arithmetic.

Permits require all callers for one resource to use the same limit. No automatic
renewal or release-on-drop occurs: expired holders cannot revive their permit,
and a short holder cannot shorten another holder's expiry. The caller must stop
protected work when ownership is lost. Random owner tokens prevent stale renewal
or release; they are not monotonic fencing tokens and cannot stop an already-running
remote side effect. Pair durable mutations with Store version/CAS checks. Redis
failover or eviction can lose coordination state: use an appropriate no-eviction
and durability configuration; this is not a consensus lock implementation.

A timeout, disconnect or cancellation after dispatch may have committed. The
adapter never blindly retries a failed mutation or switches to local memory.
redis-rs reconnects command connections for subsequent calls; a NOSCRIPT reply
can load the script because the rejected attempt did not execute it. Check state
or use durable operation receipts before retrying non-idempotent work.

## Notifications and multiple instances

```rust
# use gproxy_cache::{Cache, MemoryCache, Notification};
# async fn example() -> gproxy_cache::Result<()> {
# let cache = MemoryCache::default();
let mut changes = cache.subscribe("config").await?;
assert_eq!(changes.recv().await?, Notification::ResyncRequired);
// Reload the database snapshot + its revision now, after subscription is ready.
cache.publish("config", b"revision:42".to_vec()).await?;
# Ok(())
# }
```

`subscribe` returns after registration. Each new subscription first yields
`ResyncRequired`; an application must load authoritative data after subscribing.
Slow consumers receive `ResyncRequired` when the bounded delivery queue overruns.
Redis disconnect closes the subscription explicitly; recreate it and reload.
Dropping a Redis subscription aborts its reader task. `recv` is cancellation-safe.
Memory subscribers are notified across clones of the same cache, not separate
processes or independently constructed caches. WASM memory is isolate-local.

There is no replay, acknowledgement, exactly-once guarantee or atomicity between
Store commits and publish. Publish with zero subscribers succeeds. A process may
crash after a database commit but before publishing, so consumers must periodically
compare the durable configuration revision too. Store must update that revision
in the same transaction as configuration, and load the rows/revision consistently.
Core/manage owns that integration, revision schema, reload compilation and event
payloads; they are not implemented by this crate. See [design/cache.md](../../design/cache.md).
Do not publish decrypted credentials or bearer tokens.

## Memory capacity and limits

`MemoryOptions` defaults to 10,000 live keys and 256 active topics. `Limits`
defaults to 1,024 key bytes, 1 MiB per value/message, 10,000 permits per key, and
256 messages in each subscription delivery queue. These are count/per-item bounds,
not an allocator-level total RAM budget; configure them for the workload. Redis
also validates per-operation limits, while server-side memory capacity is managed
by Redis. Limit configuration must agree across clients sharing a namespace.

Memory never evicts a live value, counter or permit to make room: it reclaims
expired keys, then returns `Capacity` if still full. Expiry is enforced on access;
`purge_expired()` optionally reclaims cold expired entries. There is no janitor
thread or Tokio runtime requirement for Memory operations. Tokio sync is used
only for async notifications; the application drives polling.

## Validation

```sh
cargo test -p gproxy-cache
cargo clippy -p gproxy-cache --all-features --all-targets -- -D warnings
cargo clippy -p gproxy-cache --target wasm32-unknown-unknown --all-features --all-targets -- -D warnings
GPROXY_CACHE_REDIS_URL=redis://127.0.0.1:6379 cargo test -p gproxy-cache --all-features -- --include-ignored
```

Redis tests are explicitly ignored without opt-in. Supply a test server; the
fault-proxy test needs anonymous TCP Redis; the database-isolation test needs at least two databases. Tests use unique namespaces and never
FLUSHDB/FLUSHALL or kill server clients. The network-failure test closes only its
own local relay. See [VALIDATION.md](VALIDATION.md) for executed checks.
