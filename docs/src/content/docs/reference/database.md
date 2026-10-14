---
title: "Storage & Cache Backends"
description: "The database backends and how they are selected, entity-first schema synchronization, the cache contract and what needs a shared one, file storage, and backups."
---

Three storage extension points, chosen independently: the **database**, the
**cache**, and optional **file content**.

## The Database

| `GPROXY_PERSISTENCE` | Connection | Build |
| --- | --- | --- |
| `sqlite` *(default)* | `<data-dir>/gproxy.db`, or a path / `sqlite://` URL in `--dsn` | always compiled in |
| `postgres` | `postgres://…` in `--dsn` | `--features postgres` |
| `mysql` | `mysql://…` in `--dsn` | `--features mysql` |
| D1 | the Cloudflare binding | the Workers host |
| libSQL / Turso | over the Hrana HTTP pipeline | `--features libsql` |

MySQL requires 8.0.13 or later for JSON default expressions. PostgreSQL and MySQL drivers include TLS support.

`--dsn` names its own backend when `--persistence` is absent, so the scheme is
usually enough.

**A backend this build does not have is refused at startup**, naming the
feature that would provide it, rather than at the first request.

```sh
gproxy serve --dsn 'postgres://gproxy:…@db.internal:5432/gproxy'
```

libSQL is the option for a host that has no D1 — for example, Netlify — because the
HTTP half is provided by the caller's own transport and it therefore works on
every target.

### One API over all of them

Native SeaORM connections and Cloudflare D1 share **one** store API. Every
entity accessor goes through one generic repository, so ordinary CRUD is not
hand-written per table, and each method executes **one atomic batch**.

Constructing the store neither opens a database nor changes a schema.

Two consequences worth knowing:

- Reads preserve input order, duplicates and a `None` for a missing id, so a
  caller can zip a result back onto its request.
- A key predicate is split to fit D1's 100-bind statement limit **within the
  same batch**. Arbitrary caller SQL is not split, and database request and
  size limits still apply.

SQL errors roll back transactional writes. A conditional zero-row write returns
a conflict rather than rolling back the rest of its batch. **There is no
automatic retry**: an ambiguous commit result means checking durable state
first.

### Schema synchronization

```text
$ gproxy migrate --data-dir ./data
INFO gproxy::instance: schema is up to date warnings=0
```

Synchronization is **entity-first**, not a numbered migration ladder: one
registry describes the tables, and a sync creates what is missing on a new
database and incrementally adds supported missing columns and indexes on an
existing one. Repeat calls preserve rows.

`serve` does this too. The separate command exists for a deployment that runs
it as its own step, **with one writer**, before any instance starts — which is
what more than one instance over one database requires.

What synchronization does **not** do: it does not create default settings or
administrators, it does not run versioned migrations, and it does not convert a
column's type, its data, or an existing foreign key. Those need an explicit
migration, and there is no promise of one atomic transaction across every
native backend.

A report carries warnings — D1 type differences, for instance — and an empty
list **is not proof** that every change was applied. Read the diagnostics
before considering a startup complete.

### Exact amounts

Money and limits are stored as scaled integers, not floats. One atom is
`0.000000001`, and the representable range is about ±9.2 billion.

SQL columns are `BIGINT`, and the entities carry the atoms as text across D1's
JavaScript boundary so that comparisons, ordering and addition stay numeric in
the database. JSON uses decimal **strings** for the same reason.

Arithmetic happens in a decimal type and is rounded **once**, for the complete
settlement, ties to even. Settlement checks for negative values and signed
overflow before it increments a counter.

## The Cache

| Backend | Selected by | Scope |
| --- | --- | --- |
| Memory *(default natively)* | `--features memory` | one process |
| Redis / Valkey | `GPROXY_REDIS_URL` | shared |
| The database | automatic on wasm, configurable elsewhere | shared, through the same database |

The cache holds TTL state, exact counters, permits and leases, and the
invalidation topic. It holds **no** business entities and **no** durable ledger.

| Operation | Semantics |
| --- | --- |
| `get` / `put` / `delete` | opaque bytes, a mandatory positive TTL, a fresh version on every put |
| `compare_exchange` | an expected version or absence; replace or delete |
| `counter` / `increment` / `decrement` | exact non-negative integers, with an atomic ceiling check |
| `acquire_permit` | at most N live holders, each with its own TTL |
| `acquire_lease` | the same domain with a limit of 1 — a refresh lease |
| `publish` / `subscribe` | lossy invalidation hints |

Operations are atomic **per key**, never across keys. Key/value, counter,
permit and topic domains are separate, so the same logical name may exist in
each.

A counter has a **fixed window**: only its creation establishes the TTL, so
every instance agrees on a boundary from the clock alone. A permit is **not**
keyed by window — it measures requests in flight, which no boundary divides,
and its period is only how long the cache waits before reclaiming one from a
request that died without releasing it.

### Failure is a refusal, never a local fallback

A Redis failure does **not** silently degrade to the in-process cache. Two
instances each falling back would hold contradictory locks and count the same
limit twice, which is worse than an error.

A rate-limited request whose cache cannot answer is **refused with 429**.
Passing it turns a cache outage into "every limit on the instance is off" — the
one moment an attacker wants and an operator cannot see.

### What needs a shared one

Running more than one instance requires Redis, or the database-backed cache.
Without it, each instance counts its own rate limits, each refreshes the same
OAuth token, and none sees the others' configuration invalidations.

A login session also lives in the cache rather than in a process, which is what
lets any instance finish a login another one started — behind a load balancer
it usually is another one.

For a deployment with separate environments, give each its own Redis namespace.
The notification channel encodes the database number explicitly, because Redis
Pub/Sub itself ignores `SELECT`.

### The database as a cache

For a host with no memory-resident process and no Redis — an edge isolate on D1
or libSQL — the cache can be three tables in the database itself. Expiry is
compared in SQL, so peers on one database agree on what is live.

It carries **no notification transport**: a subscription yields the initial
resync hint and then nothing, so such a host catches up on its own schedule.
That is the whole of the Workers host's synchronization: one revision read at
the top of every request.

## Configuration Synchronization

```text
commit_revision([statements…, config_revision += 1, read back])   one transaction
      ↓
reload   this instance's snapshot
      ↓
publish  ConfigurationChanged { revision, scopes }
```

Two mechanisms, because neither alone is enough:

| | Carries | Fails by |
| --- | --- | --- |
| an invalidation on the shared cache | "look again", within milliseconds | losing messages |
| the `settings.config_revision` poll | the durable truth, every 30 s | being slow |

A notification never carries state. It says which revision exists, and an
instance reloads only when that is ahead of the one it serves. An unreadable
payload reloads rather than guesses.

**The cache's own counter cannot replace the database's revision.** A Redis
restart or eviction may lose it, and two independent commits — one to the
database, one to the topic — do not become a transaction by being adjacent.

Reloads are serialized and monotonic: an older revision never replaces a newer
one, and a failed reload leaves the previous snapshot serving.

## File Storage

Optional, and off unless a backend is chosen.

| Backend | Selected by | Native | Workers |
| --- | --- | --- | --- |
| Local filesystem | `--features fs` + `--file-storage-dir` | yes | no |
| S3-compatible, including R2 | `--features s3` + the `[file_storage]` block | yes | yes |

It holds published bodies — the bytes behind `/publications/{id}` — and
downloaded tokenizer vocabularies. **File metadata and ownership stay in the
database**; the storage layer holds content and nothing else.

With no file storage configured, publication and vocabulary fetching answer
"unsupported": there is nowhere to put the bytes.

A publication link also needs `GPROXY_PUBLIC_BASE_URL`. With none configured
the publish fails *before* any body is written and the caller is told to ask
for the bytes inline. Building the link from the request's `Host` — chosen by
the client — would be worse than no link: the upstream answer is accepted, the
bytes are stored, the URL is handed out, and it 404s somewhere else.

## Backups

- **SQLite** — stop the process and copy `<data-dir>/gproxy.db`, or use
  SQLite's own online backup. **Keep the master key with the copy**: a sealed
  database is unreadable without it.
- **PostgreSQL and MySQL** — the database's own dump tools.
- **D1, libSQL / Turso** — the platform's snapshots.
- **Logical** — `gproxy export --out config.json --include-secrets`. This
  carries the *configuration*, not the history: identity, usage and captures
  are deliberately out of it. See
  [Configuration](/reference/configuration/#moving-a-configuration).

A sealed blob is bound to its row's id, so restoring one row's secret onto
another row does not work — which is a safety property, not an obstacle to a
whole-database restore.
