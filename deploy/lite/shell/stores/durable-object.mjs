// Cloudflare: one Durable Object with SQLite storage holds every key.
//
// A Durable Object runs one event at a time and `compareExchange` does not
// await between its read and its write, so the exchange is atomic, and the
// storage is strongly consistent: this is a real compare-and-exchange.

import { DurableObject } from "cloudflare:workers";

export class State extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(`CREATE TABLE IF NOT EXISTS state (
      key TEXT PRIMARY KEY, value TEXT NOT NULL, version INTEGER NOT NULL, expires_at INTEGER)`);
    // Versions come from a sequence that never goes back, so a deleted and
    // recreated key has a version no reader has seen.
    this.sql.exec(`CREATE TABLE IF NOT EXISTS sequence (
      id INTEGER PRIMARY KEY CHECK (id = 1), last INTEGER NOT NULL)`);
    this.sql.exec("INSERT OR IGNORE INTO sequence (id, last) VALUES (1, 0)");
  }

  get(key) {
    const [row] = this.sql.exec("SELECT value, version FROM state WHERE key = ?", key).toArray();
    return row ? { value: row.value, version: String(row.version) } : null;
  }

  compareExchange(key, expected, value, expiresAtMs) {
    const [row] = this.sql.exec("SELECT version FROM state WHERE key = ?", key).toArray();
    if ((row ? String(row.version) : null) !== expected) return null;
    // The engine no longer reads these; drop them while writing anyway.
    this.sql.exec("DELETE FROM state WHERE expires_at < ? AND key <> ?", Date.now(), key);
    if (value === null) {
      this.sql.exec("DELETE FROM state WHERE key = ?", key);
      return { version: null };
    }
    const [{ last }] = this.sql
      .exec("UPDATE sequence SET last = last + 1 WHERE id = 1 RETURNING last")
      .toArray();
    this.sql.exec(
      "INSERT OR REPLACE INTO state (key, value, version, expires_at) VALUES (?, ?, ?, ?)",
      key, value, last, expiresAtMs,
    );
    return { version: String(last) };
  }
}

// `namespace` is the Durable Object binding of `State`.
export function durableObjectStore(namespace) {
  const state = () => namespace.get(namespace.idFromName("state"));
  return {
    get: key => state().get(key),
    compareExchange: (key, expected, value, expiresAtMs) =>
      state().compareExchange(key, expected, value, expiresAtMs),
  };
}
