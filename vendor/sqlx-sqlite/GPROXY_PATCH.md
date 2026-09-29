# GProxy SQLite batch extension

Source: crates.io `sqlx-sqlite` 0.9.0, retaining its MIT / Apache-2.0 licenses.
Upstream crate checksum: `488e99c397a62007e4229aec669a179816339afc6d2620ca6fa420dbee2e982c`.
Only this driver is patched; `sqlx-core`, SeaORM, and other drivers are unchanged.

Changes from upstream:

- `src/connection/batch.rs`: `SqliteConnection::execute_batch` sends independently
  bound statements to the existing worker, buffers one result per input, and
  reports the first failing index. Each SQL template uses the existing bounded
  statement cache. No implicit transaction or retry is added in the driver.
- `src/connection/worker.rs`: one new command and its oneshot reply; the existing
  execution iterator and cache counters are reused.
- `src/connection/mod.rs` and `src/lib.rs`: module and public type exports.
- `src/types/time.rs`: use `time` string literals and dedicated calendar-year,
  numeric-month, and 24-hour components without changing accepted date formats.
  The minimum `time` version is 0.3.55, matching the workspace lockfile.
- Three upstream Rustdoc trailing-space lines in `deserialize.rs` are normalized.

GProxy's adapter owns atomicity, ordered job retries, and cancellation semantics.
The caller moves SeaQuery values into driver arguments once, before enqueueing,
and prepares the trial copy there. SQL text uses Arc<str>; argument strings and
BLOBs share driver buffers. The drainer moves ready commands into its batch and
retains shared originals only for rollback/retry. It releases its reference before
waking the caller so normal payload cleanup runs outside the serial writer.
Scripts and numbered parameters use the same command: each input SQL retains
its own native binding scope, without concatenation or a separate lexical scan.

The root `[patch.crates-io]` pins this source. When updating SQLx, rebase these
changes and run the `gproxy-seaorm` group-commit tests (cache reuse, binding,
query results, rollback, isolation, and cancellation). The vendored package is
excluded from workspace membership to retain upstream package metadata.
