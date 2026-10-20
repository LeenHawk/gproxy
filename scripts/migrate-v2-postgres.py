#!/usr/bin/env python3
"""Data bridge for migrate-v2-postgres.sh; secrets stay in the existing importers."""

import base64
from collections import defaultdict
from decimal import Decimal, ROUND_HALF_EVEN, localcontext
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import sys

import psycopg
from psycopg import sql


def require(condition, message):
    if not condition:
        raise ValueError(message)


def quote(name):
    return '"' + name.replace('"', '""') + '"'


def key_fingerprint():
    encoded = os.environ.get("GPROXY_MASTER_KEY", "").strip()
    if not encoded:
        return None
    raw = base64.b64decode(encoded, validate=True)
    require(len(raw) == 32, "GPROXY_MASTER_KEY must be base64 for 32 bytes")
    return hashlib.sha256(raw).hexdigest()


def connect(variable):
    db = psycopg.connect(os.environ[variable])
    db.execute("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
    return db


def sqlite_read(path):
    return sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)


def columns(db, table):
    return {row[1] for row in db.execute(f"PRAGMA table_info({quote(table)})")}


def count(db, table):
    return db.execute(f"SELECT count(*) FROM {quote(table)}").fetchone()[0]


def snapshot(work):
    fingerprint = key_fingerprint()
    schema = os.environ.get("GPROXY_V2_SCHEMA", "public")
    with connect("GPROXY_V2_SOURCE_DSN") as source:
        tables = defaultdict(list)
        for table, column, kind in source.execute(
            "SELECT c.table_name,c.column_name,c.udt_name "
            "FROM information_schema.columns c JOIN information_schema.tables t "
            "USING (table_catalog,table_schema,table_name) "
            "WHERE c.table_schema=%s AND t.table_type='BASE TABLE' "
            "ORDER BY c.table_name,c.ordinal_position", (schema,)
        ):
            tables[table].append((column, kind))
        require("seaql_migrations" not in tables, "Source contains v4 tables; restore the original v2 backup first")
        required = {"providers": "channel", "credentials": "secret_json",
                    "users": "password", "user_keys": "api_key_ciphertext",
                    "usages": "cost", "price_rules": "input_price"}
        for table, column in required.items():
            require(column in dict(tables.get(table, [])), f"Not a v2 schema: missing {table}.{column}")
        counts = {}
        path = work / "v2.db"
        path.touch(mode=0o600, exist_ok=False)
        with sqlite3.connect(path) as target:
            for table, fields in tables.items():
                definitions, selections = [], []
                for name, kind in fields:
                    if kind in ("int2", "int4", "int8", "bool"):
                        storage = "INTEGER"
                    elif kind == "bytea":
                        storage = "BLOB"
                    elif kind in ("text", "varchar", "bpchar", "json", "jsonb", "numeric"):
                        storage = "TEXT"
                    elif kind in ("float4", "float8"):
                        storage = "REAL"
                    else:
                        raise ValueError(f"Unsupported source type: {table}.{name} ({kind})")
                    primary = " PRIMARY KEY" if name == "id" and kind in ("int2", "int4", "int8") else ""
                    definitions.append(f"{quote(name)} {storage}{primary}")
                    field = sql.Identifier(name)
                    selections.append(sql.SQL("{}::text").format(field) if storage == "TEXT" else field)
                target.execute(f"CREATE TABLE {quote(table)} ({','.join(definitions)})")
                insert = f"INSERT INTO {quote(table)} VALUES ({','.join('?' for _ in fields)})"
                copied = 0
                with source.cursor(name="snapshot_rows") as cursor:
                    cursor.execute(sql.SQL("SELECT {} FROM {}.{}").format(
                        sql.SQL(",").join(selections), sql.Identifier(schema), sql.Identifier(table)))
                    while rows := cursor.fetchmany(1000):
                        target.executemany(insert, rows)
                        copied += len(rows)
                expected = source.execute(sql.SQL("SELECT count(*) FROM {}.{}").format(
                    sql.Identifier(schema), sql.Identifier(table))).fetchone()[0]
                require(copied == expected == count(target, table), f"Row count mismatch: {table}")
                counts[table] = copied
            adjustments = normalize_v2(target)
            require(target.execute("PRAGMA integrity_check").fetchone()[0] == "ok", "SQLite integrity check failed")
    return {"source_schema": schema, "rows": counts, "adjustments": adjustments,
            "master_key_sha256": fingerprint}


def normalize_v2(db):
    """Add only fields absent in v2.3.2 that the v3.0.22 reader requires."""
    changes = []
    additions = {
        "usages": {"thread_id": "TEXT", "image_output_tokens": "INTEGER NOT NULL DEFAULT 0",
                   "metrics_json": "TEXT NOT NULL DEFAULT '{}'"},
        "quotas": {"quota_5h": "TEXT", "quota_7d": "TEXT"},
        "rules": {"filter_header_pattern": "TEXT"},
        "instance_settings": {"enable_auto_update_check": "INTEGER NOT NULL DEFAULT 1",
                              "file_upload_max_in_flight": "INTEGER NOT NULL DEFAULT 0"},
        "price_rules": {"image_output_price": "TEXT NOT NULL DEFAULT '0'", "pricing_tiers_json": "TEXT"},
    }
    for table, fields in additions.items():
        existing = columns(db, table)
        require(existing, f"Missing required v2 table: {table}")
        for name, definition in fields.items():
            if name not in existing:
                db.execute(f"ALTER TABLE {quote(table)} ADD COLUMN {quote(name)} {definition}")
                changes.append(f"Added {table}.{name}: {definition}")
    db.execute("CREATE TABLE IF NOT EXISTS price_rule_rates (id INTEGER PRIMARY KEY, "
               "price_rule_id INTEGER, metric TEXT, unit_size INTEGER, price_usd TEXT, "
               "conditions_json TEXT, sort_order INTEGER)")
    if "image_price" in columns(db, "price_rules"):
        # An explicit rate set stops v3 from synthesizing its token-only rates.
        # Preserve v2's six token rates AND its per-image price, without floats.
        metrics = ["input_tokens", "output_tokens", "cache_read_tokens", "cache_creation_5m_tokens",
                   "cache_creation_30m_tokens", "cache_creation_1h_tokens", "image_outputs"]
        prices = "input_price,output_price,cache_read_price,cache_creation_5m_price,cache_creation_30m_price,cache_creation_1h_price,image_price"
        for row in db.execute(f"SELECT id,{prices} FROM price_rules").fetchall():
            require(not db.execute("SELECT 1 FROM price_rule_rates WHERE price_rule_id=?", (row[0],)).fetchone(),
                    "Ambiguous legacy image_price alongside explicit price_rule_rates")
            for index, (metric, price) in enumerate(zip(metrics, row[1:])):
                db.execute("INSERT INTO price_rule_rates(price_rule_id,metric,unit_size,price_usd,sort_order) VALUES(?,?,?,?,?)",
                           (row[0], metric, 1 if metric == "image_outputs" else 1_000_000, price, index))
        changes.append("Preserved legacy image_price as image_outputs per image, alongside token rates per million")
    return changes


def usage_totals(db, table):
    with localcontext() as context:
        context.prec = 80
        costs = sum((Decimal(row[0]) for row in db.execute(f"SELECT cost FROM {quote(table)}")), Decimal(0))
    return {"rows": count(db, table), "cost": str(costs)}


def verify_v3(work):
    with sqlite_read(work / "v2.db") as source, sqlite_read(work / "v3/gproxy.db") as target:
        require(target.execute("PRAGMA integrity_check").fetchone()[0] == "ok", "v3 integrity check failed")
        require(not target.execute("PRAGMA foreign_key_check").fetchone(), "v3 foreign key check failed")
        counts = {}
        for old, new in [("providers", "providers"), ("credentials", "credentials"),
                         ("routes", "routes"), ("orgs", "organizations"), ("teams", "teams"),
                         ("users", "users"), ("user_keys", "user_keys")]:
            before, after = count(source, old), count(target, new)
            require(after >= before, f"Lost {old} rows in v3; inspect v3-import.log")
            counts[old] = {"v2": before, "v3": after}
        before, after = usage_totals(source, "usages"), usage_totals(target, "usage_rows")
        require(before["rows"] == after["rows"] and Decimal(before["cost"]) == Decimal(after["cost"]),
                "v3 usage row count or exact settled cost changed")
        for name, password in source.execute("SELECT name,password FROM users"):
            migrated = target.execute("SELECT password_hash FROM users WHERE name=?", (name,)).fetchone()
            require(migrated == (password,), "A user password hash changed in v3")
        return {"configuration": counts, "usage": after}


def check_target(work):
    manifest = json.loads((work / "snapshot-report.json").read_text())
    require(manifest["master_key_sha256"] == key_fingerprint(), "Use the same GPROXY_MASTER_KEY as prepare")
    with connect("GPROXY_V4_TARGET_DSN") as target:
        require(target.execute("SELECT current_schema()").fetchone()[0] == "public",
                "Target must use the public schema in a dedicated empty database")
        existing = target.execute("SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace "
                                  "WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema' "
                                  "AND c.relkind IN ('r','p','v','m','S','f') LIMIT 1").fetchone()
        require(not existing, "Target is not empty; use a new database, not the original or a partial import")
    return {"target_empty": True}


def verify_v4(work):
    with sqlite_read(work / "v3/gproxy.db") as source, connect("GPROXY_V4_TARGET_DSN") as target:
        counts = {}
        for old, new in [("providers", "providers"), ("credentials", "credentials"),
                         ("routes", "routes"), ("organizations", "organizations"),
                         ("teams", "teams"), ("users", "users"), ("user_keys", "api_keys")]:
            expected = {f"v3-{old}-{row[0]}" for row in source.execute(f"SELECT id FROM {quote(old)}")}
            actual = {row[0] for row in target.execute(sql.SQL("SELECT id FROM {}").format(sql.Identifier(new)))}
            require(expected <= actual, f"Lost {old} IDs in v4; inspect v4-import.log")
            counts[new] = len(actual)
        for user_id, password in source.execute("SELECT id,password_hash FROM users"):
            migrated = target.execute("SELECT password_hash FROM users WHERE id=%s", (f"v3-users-{user_id}",)).fetchone()
            require(migrated == (password,), "A user password hash changed in v4")
        for key_id, digest in source.execute("SELECT id,digest FROM user_keys"):
            migrated = target.execute("SELECT key_hash FROM api_keys WHERE id=%s", (f"v3-user_keys-{key_id}",)).fetchone()
            require(migrated == (digest.hex(),), "An API key digest changed in v4")
        expected = usage_totals(source, "usage_rows")
        rows, total = 0, Decimal(0)
        with localcontext() as context, target.cursor(name="verify_usage") as cursor:
            context.prec = 80
            cursor.execute("SELECT request_id,metrics,cost FROM usage_records")
            for request_id, metrics, stored_cost in cursor:
                original = metrics["v3"]
                row = source.execute("SELECT request_id,cost FROM usage_rows WHERE id=?", (original["id"],)).fetchone()
                require(row is not None and row[0] == original["request_id"]
                        and Decimal(row[1]) == Decimal(original["cost"])
                        and request_id == f"v3-usage_rows-{original['id']}", "Historical usage identity or exact cost changed")
                rounded = Decimal(original["cost"]).quantize(Decimal("0.000000001"), rounding=ROUND_HALF_EVEN)
                require(stored_cost == rounded * 1_000_000_000, "v4 stored cost differs from the rounded original")
                rows += 1
                total += Decimal(original["cost"])
        require(rows == expected["rows"] and total == Decimal(expected["cost"]), "v4 usage count or original cost changed")
        return {"configuration": counts, "usage_rows": rows, "original_cost": str(total),
                "note": "v4 stores costs at 9 decimal places; exact originals are retained in metrics.v3.cost. Review both import logs for semantic changes and skipped data."}


if __name__ == "__main__":
    os.umask(0o077)
    try:
        action, directory = sys.argv[1:]
        result = {"snapshot": snapshot, "verify-v3": verify_v3,
                  "check-target": check_target, "verify-v4": verify_v4}[action](Path(directory).resolve())
        print(json.dumps(result, indent=2))
    except Exception as error:
        # Connection errors can include credentials; never print the DSN.
        message = str(error)
        for variable in ("GPROXY_V2_SOURCE_DSN", "GPROXY_V4_TARGET_DSN", "GPROXY_MASTER_KEY"):
            secret = os.environ.get(variable)
            if secret:
                message = message.replace(secret, "<redacted>")
        print(f"Migration failed: {message}", file=sys.stderr)
        sys.exit(1)
