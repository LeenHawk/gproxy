#!/usr/bin/env python3
"""Offline migration: any supported v2 database -> SQLite -> v3 -> v4 -> target."""
import argparse
import base64
from collections import defaultdict
from contextlib import closing, contextmanager
from decimal import Decimal, localcontext, ROUND_HALF_EVEN
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import ssl
import subprocess
import sys
import tempfile
from urllib.parse import parse_qsl, unquote, urlsplit


class Database:
    """The two database-copy boundaries; version upgrades stay in the native CLIs."""
    def __init__(self, dsn, readonly=True, schema=None):
        self.kind, self.dsn = endpoint(dsn)
        self.schema = schema or "public"
        self.serial = 0
        if self.kind == "sqlite":
            self.connection = sqlite3.connect(self.dsn.as_uri() + "?mode=ro", uri=True)
        elif self.kind == "postgres":
            try:
                import psycopg
            except ImportError:
                raise ValueError("PostgreSQL requires: python3 -m pip install 'psycopg[binary]>=3,<4'") from None
            self.connection = psycopg.connect(self.dsn)
            self.execute("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ" + (" READ ONLY" if readonly else ""))
            if schema is None:
                require(self.one("SELECT current_schema()")[0] == "public", "PostgreSQL target must use the public schema")
        else:
            try:
                import pymysql
            except ImportError:
                raise ValueError("MySQL requires: python3 -m pip install 'PyMySQL[rsa]>=1,<2'") from None
            url = urlsplit(self.dsn)
            options = dict(parse_qsl(url.query))
            require(set(options) <= {"charset", "ssl-mode", "ssl-ca", "ssl-cert", "ssl-key"},
                    "Unsupported MySQL DSN options; use charset, ssl-mode, ssl-ca, ssl-cert or ssl-key")
            mode = options.pop("ssl-mode", "PREFERRED").upper().replace("-", "_")
            require(mode in ("DISABLED", "PREFERRED", "REQUIRED", "VERIFY_CA", "VERIFY_IDENTITY"), "Unsupported MySQL ssl-mode")
            if mode != "DISABLED":
                context = ssl.create_default_context(cafile=options.pop("ssl-ca", None))
                context.check_hostname = mode == "VERIFY_IDENTITY"
                if mode not in ("VERIFY_CA", "VERIFY_IDENTITY"):
                    context.verify_mode = ssl.CERT_NONE
                certificate, private_key = options.pop("ssl-cert", None), options.pop("ssl-key", None)
                if certificate:
                    context.load_cert_chain(certificate, private_key)
                options["ssl"] = context
            else:
                require(not any(key.startswith("ssl-") for key in options), "ssl-mode=DISABLED cannot use TLS certificates")
            self.schema = unquote(url.path.lstrip("/"))
            require(self.schema, "MySQL DSN must name a database")
            self.connection = pymysql.connect(host=url.hostname or "localhost", port=url.port or 3306,
                user=unquote(url.username or ""), password=unquote(url.password or ""),
                database=self.schema, autocommit=False, **options)
            if mode in ("REQUIRED", "VERIFY_CA", "VERIFY_IDENTITY"):
                if not self.one("SHOW SESSION STATUS LIKE 'Ssl_cipher'")[1]:
                    self.connection.close()
                    raise ValueError("MySQL server did not establish the requested TLS connection")
            self.execute("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            self.execute("START TRANSACTION WITH CONSISTENT SNAPSHOT" + (", READ ONLY" if readonly else ""))

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.connection.close()

    def quote(self, name):
        mark = "`" if self.kind == "mysql" else '"'
        return mark + name.replace(mark, mark * 2) + mark

    def table(self, name):
        return self.quote(name) if self.kind == "sqlite" else self.quote(self.schema) + "." + self.quote(name)

    def execute(self, statement, params=()):
        with closing(self.connection.cursor()) as cursor:
            if params:
                cursor.execute(statement.replace("?", "%s") if self.kind != "sqlite" else statement, params)
            else:
                cursor.execute(statement)

    def rows(self, statement, params=()):
        if self.kind == "postgres":
            self.serial += 1
            cursor = self.connection.cursor(name=f"migration_{self.serial}")
        elif self.kind == "mysql":
            import pymysql
            cursor = self.connection.cursor(pymysql.cursors.SSCursor)
        else:
            cursor = self.connection.cursor()
        with closing(cursor):
            if params:
                cursor.execute(statement.replace("?", "%s") if self.kind != "sqlite" else statement, params)
            else:
                cursor.execute(statement)
            while batch := cursor.fetchmany(1000):
                yield from batch

    def one(self, statement, params=()):
        with closing(self.rows(statement, params)) as rows:
            return next(rows)

    def tables(self):
        if self.kind == "sqlite":
            names = [r[0] for r in self.rows("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
            return {name: {r[1]: r[2].lower() for r in self.rows(f"PRAGMA table_info({self.quote(name)})")} for name in names}
        column_type = "c.udt_name" if self.kind == "postgres" else "c.data_type"
        tables = defaultdict(dict)
        for table, column, kind in self.rows(
            f"SELECT c.table_name,c.column_name,{column_type} FROM information_schema.columns c "
            "JOIN information_schema.tables t USING (table_catalog,table_schema,table_name) "
            "WHERE c.table_schema=? AND t.table_type='BASE TABLE' ORDER BY c.table_name,c.ordinal_position", (self.schema,)):
            tables[table][column] = kind
        return dict(tables)

    def insert(self, table, columns, rows):
        marks = ",".join("%s" for _ in columns)
        statement = f"INSERT INTO {self.table(table)} ({','.join(map(self.quote, columns))}) VALUES ({marks})"
        with closing(self.connection.cursor()) as cursor:
            cursor.executemany(statement, rows)



def endpoint(value):
    if value.startswith(("postgres://", "postgresql://")):
        return "postgres", value
    if value.startswith("mysql://"):
        return "mysql", value
    require("://" not in value or value.startswith("sqlite://"), "Expected a SQLite file, sqlite://, postgres:// or mysql:// DSN")
    path = value.removeprefix("sqlite://").split("?", 1)[0]
    require(path and path not in (":memory:", "sqlite::memory:"), "SQLite must be a persistent file")
    return "sqlite", Path(path).expanduser().resolve()



def quote(name):
    return '"' + name.replace('"', '""') + '"'


@contextmanager


def sqlite_read(path):
    with closing(sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)) as connection:
        yield connection



def require(condition, message):
    if not condition:
        raise ValueError(message)


def check_master_key():
    encoded = os.environ.get("GPROXY_MASTER_KEY", "").strip()
    if not encoded:
        return None
    raw = base64.b64decode(encoded, validate=True)
    require(len(raw) == 32, "GPROXY_MASTER_KEY must be base64 for 32 bytes")


def columns(db, table):
    return {row[1] for row in db.execute(f"PRAGMA table_info({quote(table)})")}


def count(db, table):
    return db.execute(f"SELECT count(*) FROM {quote(table)}").fetchone()[0]


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


def validate_v2(tables):
    require("seaql_migrations" not in tables, "Source is already v4; use the original v2 database")
    for table, column in {"providers": "channel", "credentials": "secret_json", "users": "password",
                          "user_keys": "api_key_ciphertext", "usages": "cost", "price_rules": "input_price"}.items():
        require(column in tables.get(table, {}), f"Not a supported v2 schema: missing {table}.{column}")



def snapshot(dsn, schema, work):
    path = work / "v2.db"
    with Database(dsn, schema=schema) as source:
        tables = source.tables()
        validate_v2(tables)
        if source.kind == "sqlite":
            with closing(sqlite3.connect(path)) as target:
                source.connection.backup(target)
        else:
            if source.kind == "mysql":
                engines = list(source.rows("SELECT table_name,engine FROM information_schema.tables WHERE table_schema=? AND table_type='BASE TABLE'", (source.schema,)))
                require(all(engine == "InnoDB" for _, engine in engines), "MySQL source tables must use InnoDB for a consistent snapshot")
            with closing(sqlite3.connect(path)) as target, target:
                for table, fields in tables.items():
                    definitions, selections = [], []
                    for name, kind in fields.items():
                        if kind in ("int2", "int4", "int8", "bool", "tinyint", "smallint", "mediumint", "int", "bigint"):
                            storage = "INTEGER"
                        elif kind in ("bytea", "binary", "varbinary", "tinyblob", "blob", "mediumblob", "longblob"):
                            storage = "BLOB"
                        elif kind in ("text", "varchar", "bpchar", "char", "tinytext", "mediumtext", "longtext", "json", "jsonb", "numeric", "decimal", "enum"):
                            storage = "TEXT"
                        elif kind in ("float4", "float8", "float", "double"):
                            storage = "REAL"
                        else:
                            raise ValueError(f"Unsupported v2 column type: {table}.{name} ({kind})")
                        primary = " PRIMARY KEY" if name == "id" and storage == "INTEGER" else ""
                        definitions.append(f"{quote(name)} {storage}{primary}")
                        field = source.quote(name)
                        selections.append(f"CAST({field} AS {'TEXT' if source.kind == 'postgres' else 'CHAR'})" if storage == "TEXT" else field)
                    target.execute(f"CREATE TABLE {quote(table)} ({','.join(definitions)})")
                    insert = f"INSERT INTO {quote(table)} VALUES ({','.join('?' for _ in fields)})"
                    batch = []
                    for row in source.rows(f"SELECT {','.join(selections)} FROM {source.table(table)}"):
                        batch.append(row)
                        if len(batch) == 1000:
                            target.executemany(insert, batch)
                            batch.clear()
                    target.executemany(insert, batch)
                    expected = source.one(f"SELECT count(*) FROM {source.table(table)}")[0]
                    require(count(target, table) == expected, f"Snapshot row count mismatch: {table}")
    with closing(sqlite3.connect(path)) as snapshot_db, snapshot_db:
        counts = {table: count(snapshot_db, table) for table in tables}
        changes = normalize_v2(snapshot_db)
        require(snapshot_db.execute("PRAGMA integrity_check").fetchone()[0] == "ok", "Snapshot integrity check failed")
    return {"source_kind": endpoint(dsn)[0], "rows": counts, "adjustments": changes}



def check_target(dsn):
    kind, target = endpoint(dsn)
    if kind == "sqlite":
        require(not target.exists(), "SQLite target must be a new file")
        return
    with Database(dsn) as db:
        if kind == "postgres":
            exists = db.one("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace "
                            "WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema' "
                            "AND c.relkind IN ('r','p','v','m','S','f'))")[0]
        else:
            exists = db.one("SELECT count(*) FROM information_schema.tables WHERE table_schema=?", (db.schema,))[0]
        require(not exists, "Target is not empty; select a new database, never the running v2 database")



def portable(value, kind):
    if value is None:
        return None
    if kind in ("json", "jsonb"):
        return json.loads(value) if isinstance(value, str) else value
    if kind in ("bool", "boolean"):
        return int(value)
    if kind in ("numeric", "decimal"):
        number = Decimal(str(value))
        with localcontext() as context:
            context.prec = max(80, len(number.as_tuple().digits))
            return str(number.normalize())
    if isinstance(value, (bytes, memoryview)):
        return {"base64": base64.b64encode(value).decode()}
    return value



def table_digest(db, table, fields):
    total, size = 0, 0
    names = ",".join(map(db.quote, fields))
    for row in db.rows(f"SELECT {names} FROM {db.table(table)}"):
        values = [portable(value, kind) for value, kind in zip(row, fields.values())]
        encoded = json.dumps(values, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()
        total = (total + int.from_bytes(hashlib.sha256(encoded).digest(), "big")) % (1 << 256)
        size += 1
    return {"rows": size, "row_sha256_sum": f"{total:064x}"}



def table_order(db, tables):
    if db.kind == "postgres":
        edges = db.rows("SELECT t.relname,p.relname FROM pg_constraint c "
                        "JOIN pg_class t ON t.oid=c.conrelid JOIN pg_class p ON p.oid=c.confrelid "
                        "JOIN pg_namespace n ON n.oid=t.relnamespace WHERE c.contype='f' AND n.nspname=?", (db.schema,))
    else:
        edges = db.rows("SELECT table_name,referenced_table_name FROM information_schema.key_column_usage "
                        "WHERE table_schema=? AND referenced_table_name IS NOT NULL", (db.schema,))
    pending = {table: set() for table in tables}
    for child, parent in edges:
        if child in pending and parent in pending and child != parent:
            pending[child].add(parent)
    result = []
    while pending:
        ready = sorted(table for table, parents in pending.items() if not parents)
        require(ready, "Target schema has cyclic foreign keys; refusing to disable integrity checks")
        for table in ready:
            del pending[table]
        for parents in pending.values():
            parents.difference_update(ready)
        result.extend(ready)
    return result



def copy_v4(source_path, target_dsn):
    kind, target_path = endpoint(target_dsn)
    if kind == "sqlite":
        target_path.parent.mkdir(parents=True, exist_ok=True)
        require(not target_path.exists(), "SQLite target must be a new file")
        with target_path.open("xb"):
            pass
        with sqlite_read(source_path) as source, closing(sqlite3.connect(target_path)) as target:
            source.backup(target)
            require(target.execute("PRAGMA integrity_check").fetchone()[0] == "ok", "Target SQLite integrity check failed")
        return {"kind": "sqlite", "database": str(target_path)}
    with Database(str(source_path)) as source, Database(target_dsn, readonly=False) as target:
        original, destination = source.tables(), target.tables()
        require(original.keys() == destination.keys(), "v4 source and target table sets differ; use the same v4 binary")
        for table in original:
            require(original[table].keys() == destination[table].keys(), f"v4 columns differ: {table}")
        require(list(source.rows('SELECT version FROM seaql_migrations ORDER BY version')) ==
                list(target.rows('SELECT version FROM seaql_migrations ORDER BY version')), "v4 migration ledgers differ")
        tables = {name: fields for name, fields in destination.items() if name != "seaql_migrations"}
        order = table_order(target, tables)
        for table in reversed(order):
            target.execute(f"DELETE FROM {target.table(table)}")
        reports = {}
        for table in order:
            fields = tables[table]
            batch = []
            for row in source.rows(f"SELECT {','.join(map(quote, fields))} FROM {quote(table)}"):
                values = []
                for value, field_type in zip(row, fields.values()):
                    if value is not None and field_type in ("json", "jsonb"):
                        if target.kind == "postgres":
                            from psycopg.types.json import Json, Jsonb
                            value = (Jsonb if field_type == "jsonb" else Json)(value, dumps=lambda raw: raw)
                    elif value is not None and field_type == "bool":
                        value = bool(value)
                    values.append(value)
                batch.append(values)
                if len(batch) == 1000:
                    target.insert(table, fields, batch)
                    batch.clear()
            if batch:
                target.insert(table, fields, batch)
            before = table_digest(source, table, fields)
            after = table_digest(target, table, fields)
            require(before == after, f"v4 database copy verification failed: {table}")
            reports[table] = after
        if target.kind == "postgres":
            for table, fields in tables.items():
                for column in fields:
                    sequence = target.one("SELECT pg_get_serial_sequence(?,?)", (target.table(table), column))[0]
                    if sequence:
                        maximum = target.one(f"SELECT COALESCE(MAX({target.quote(column)}),0) FROM {target.table(table)}")[0]
                        target.execute("SELECT setval(?::regclass,?,false)", (sequence, maximum + 1))
        target.connection.commit()
        return {"kind": kind, "tables": reports}



def verify_v4(work):
    with sqlite_read(work / "v3/gproxy.db") as source, sqlite_read(work / "v4/gproxy.db") as target:
        require(target.execute("PRAGMA integrity_check").fetchone()[0] == "ok", "v4 integrity check failed")
        require(not target.execute("PRAGMA foreign_key_check").fetchone(), "v4 foreign key check failed")
        counts = {}
        for old, new in [("providers", "providers"), ("credentials", "credentials"), ("routes", "routes"),
                         ("organizations", "organizations"), ("teams", "teams"), ("users", "users"), ("user_keys", "api_keys")]:
            expected = {f"v3-{old}-{row[0]}" for row in source.execute(f"SELECT id FROM {quote(old)}")}
            actual = {row[0] for row in target.execute(f"SELECT id FROM {quote(new)}")}
            require(expected <= actual, f"Lost {old} IDs in v4")
            counts[new] = len(actual)
        for user_id, password in source.execute("SELECT id,password_hash FROM users"):
            require(target.execute("SELECT password_hash FROM users WHERE id=?", (f"v3-users-{user_id}",)).fetchone() == (password,), "Password hash changed in v4")
        for key_id, digest in source.execute("SELECT id,digest FROM user_keys"):
            require(target.execute("SELECT key_hash FROM api_keys WHERE id=?", (f"v3-user_keys-{key_id}",)).fetchone() == (digest.hex(),), "API key digest changed in v4")
        expected = usage_totals(source, "usage_rows")
        total, rows = Decimal(0), 0
        with localcontext() as context:
            context.prec = 80
            for request_id, metrics, cost in target.execute("SELECT request_id,metrics,cost FROM usage_records"):
                original = json.loads(metrics)["v3"]
                old = source.execute("SELECT request_id,cost FROM usage_rows WHERE id=?", (original["id"],)).fetchone()
                require(old is not None and old[0] == original["request_id"] and Decimal(old[1]) == Decimal(original["cost"])
                        and request_id == f"v3-usage_rows-{original['id']}", "Historical usage identity or cost changed")
                rounded = Decimal(original["cost"]).quantize(Decimal("0.000000001"), rounding=ROUND_HALF_EVEN)
                require(cost == rounded * 1_000_000_000, "v4 rounded cost mismatch")
                rows += 1
                total += Decimal(original["cost"])
        require(rows == expected["rows"] and total == Decimal(expected["cost"]), "v4 usage count or exact historical cost changed")
        return {"configuration": counts, "usage_rows": rows, "original_cost": str(total)}



def binary_path(value):
    found = shutil.which(value)
    require(found is not None, f"Executable not found: {value}")
    return str(Path(found).resolve())



def run(binary, args, work, report=None, target=None):
    environment = {name: os.environ[name] for name in ("PATH", "HOME", "SYSTEMROOT", "TEMP", "TMP") if name in os.environ}
    environment["LANG"] = "C.UTF-8"
    if os.environ.get("GPROXY_MASTER_KEY"):
        environment["GPROXY_MASTER_KEY"] = os.environ["GPROXY_MASTER_KEY"]
        environment["GPROXY_IMPORT_SOURCE_MASTER_KEY"] = os.environ["GPROXY_MASTER_KEY"]
    if target:
        environment["GPROXY_DSN"] = str(endpoint(target)[1])
    if report:
        with (work / report).open("w") as log:
            result = subprocess.run([binary, *args], cwd=work, env=environment, stdout=log, stderr=subprocess.STDOUT)
        require(result.returncode == 0, f"Migration stage failed; inspect {work / report}")
        return ""
    result = subprocess.run([binary, *args], cwd=work, env=environment, capture_output=True, text=True)
    require(result.returncode == 0, "Could not read the migration binary version")
    return result.stdout.strip()



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default=os.environ.get("GPROXY_SOURCE_DSN"), help="v2 SQLite file or PostgreSQL/MySQL DSN; alternatively GPROXY_SOURCE_DSN")
    parser.add_argument("--target", default=os.environ.get("GPROXY_TARGET_DSN"), help="new v4 SQLite file or empty PostgreSQL/MySQL database; alternatively GPROXY_TARGET_DSN")
    parser.add_argument("--v3-bin", default="gproxy-v3", help="v3.0.22 executable (default: gproxy-v3 on PATH)")
    parser.add_argument("--v4-bin", default="gproxy", help="v4 executable used for both staging and target schemas (default: gproxy on PATH)")
    parser.add_argument("--source-schema", default="public", help="PostgreSQL source schema (default: public)")
    parser.add_argument("--work-dir", help="new directory to retain all intermediate databases and reports")
    args = parser.parse_args()
    if not args.source or not args.target:
        parser.error("provide --source/--target or GPROXY_SOURCE_DSN/GPROXY_TARGET_DSN")
    secrets = [args.source, args.target, os.environ.get("GPROXY_MASTER_KEY", "")]
    for dsn in (args.source, args.target):
        if "://" in dsn and urlsplit(dsn).password:
            secrets.append(unquote(urlsplit(dsn).password))
    work = None
    try:
        os.umask(0o077)
        check_master_key()
        require(endpoint(args.source) != endpoint(args.target), "Source and target must be different databases")
        check_target(args.target)
        v3, v4 = binary_path(args.v3_bin), binary_path(args.v4_bin)
        work = Path(args.work_dir).resolve() if args.work_dir else Path(tempfile.mkdtemp(prefix="gproxy-v2-to-v4-", dir=Path.cwd()))
        if args.work_dir:
            work.mkdir(mode=0o700, parents=True, exist_ok=False)
        require(run(v3, ["--version"], work).split()[:2] == ["gproxy", "3.0.22"], "--v3-bin must be gproxy 3.0.22")
        require(run(v4, ["--version"], work).startswith("gproxy 4."), "--v4-bin must be a v4 binary")
        def report(name, value):
            (work / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
        print(f"1/4 Snapshot v2 -> SQLite. Work directory: {work}", flush=True)
        report("snapshot-report.json", snapshot(args.source, args.source_schema, work))
        print("2/4 Upgrade SQLite -> v3", flush=True)
        (work / "v3").mkdir()
        options = ["--persistence", "sqlite", "--data-dir", str(work / "v3"), "migrate", "--from-v2", str(work / "v2.db")]
        if os.environ.get("GPROXY_MASTER_KEY"):
            options += ["--from-v2-master-key", os.environ["GPROXY_MASTER_KEY"]]
        run(v3, options, work, "v3-preflight.log")
        run(v3, [*options, "--apply"], work, "v3-import.log")
        report("v3-verification.json", verify_v3(work))
        print("3/4 Upgrade SQLite -> v4", flush=True)
        run(v4, ["--persistence", "sqlite", "--data-dir", str(work / "v4"), "import", "--from-v3", str(work / "v3/gproxy.db")], work, "v4-import.log")
        report("v4-verification.json", verify_v4(work))
        print(f"4/4 Copy v4 SQLite -> {endpoint(args.target)[0]}", flush=True)
        check_target(args.target)
        if endpoint(args.target)[0] != "sqlite":
            run(v4, ["--data-dir", str(work / "target"), "migrate"], work, "target-schema.log", args.target)
        report("target-verification.json", copy_v4(work / "v4/gproxy.db", args.target))
        print(f"Migration complete. Review reports in {work}; start v4 against the target with the same master key.")
    except Exception as error:
        message = str(error)
        for secret in sorted(filter(None, secrets), key=len, reverse=True):
            message = message.replace(secret, "<redacted>")
        print(f"Migration failed: {message}", file=sys.stderr)
        if work:
            print(f"Intermediate databases and logs retained in {work}. No source changes or automatic target cleanup.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
