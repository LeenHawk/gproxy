#!/usr/bin/env bash
# Offline v2 PostgreSQL -> v3 SQLite -> v4 PostgreSQL migration.
set -euo pipefail
umask 077

usage() {
  cat <<'EOF'
Usage:
  bash scripts/migrate-v2-postgres.sh prepare WORK_DIR /path/to/gproxy-v3.0.22
  bash scripts/migrate-v2-postgres.sh import  WORK_DIR /path/to/gproxy-v4

prepare reads GPROXY_V2_SOURCE_DSN into a NEW private work directory and
builds/validates a v3 SQLite database. It never writes to PostgreSQL.
import writes that prepared database to an EMPTY GPROXY_V4_TARGET_DSN.

Environment:
  GPROXY_MASTER_KEY       Original v2 base64 master key; omit for plaintext
  GPROXY_V2_SOURCE_DSN    PostgreSQL URI (prefer a restored backup)
  GPROXY_V2_SCHEMA        Source schema, default public
  GPROXY_V4_TARGET_DSN    PostgreSQL URI for a new, dedicated empty database
  PYTHON                 Python with psycopg 3 installed, default python3

Stop all GPROXY writers and retain a pg_dump backup before preparing.
Review the reports before importing. See docs/v2-postgres-upgrade.md.
EOF
}

if [[ ${1:-} == --help || ${1:-} == -h ]]; then usage; exit 0; fi
if [[ $# != 3 || ( $1 != prepare && $1 != import ) ]]; then usage >&2; exit 2; fi
action=$1
work=$2
python=${PYTHON:-python3}
python=$(command -v -- "$python")
python=$("$python" -c 'import sys; print(sys.executable)')
helper=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/migrate-v2-postgres.py
binary=$("$python" -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve(strict=True))' "$3")
[[ -x $binary ]] || { echo 'GPROXY binary is not executable' >&2; exit 1; }
"$python" -c 'import psycopg' || { echo 'Install psycopg: python3 -m pip install "psycopg[binary]>=3,<4"' >&2; exit 1; }

# No inherited GPROXY config, rotation settings, .env, proxy or bootstrap.
# Neither command serves HTTP or starts an ordinary gateway instance.
run_gproxy() {
  local environment=(env -i PATH="$PATH" HOME="$HOME" LANG=C.UTF-8)
  if [[ -n ${GPROXY_MASTER_KEY:-} ]]; then
    environment+=(GPROXY_MASTER_KEY="$GPROXY_MASTER_KEY"
      GPROXY_IMPORT_SOURCE_MASTER_KEY="$GPROXY_MASTER_KEY")
  fi
  if [[ -n ${migration_dsn:-} ]]; then environment+=(GPROXY_DSN="$migration_dsn"); fi
  "${environment[@]}" "$binary" "$@"
}

run_logged() {
  local report=$1
  shift
  run_gproxy "$@" > "$report" 2>&1 || {
    echo "GPROXY failed; inspect $work/$report" >&2
    return 1
  }
}

version=$(run_gproxy --version)
case "$action:$version" in
  'prepare:gproxy 3.0.22'*) ;;
  'import:gproxy 4.'*) ;;
  *) echo 'prepare requires v3.0.22; import requires a v4 binary with PostgreSQL support' >&2; exit 1 ;;
esac

if [[ $action == prepare ]]; then
  : "${GPROXY_V2_SOURCE_DSN:?Set GPROXY_V2_SOURCE_DSN}"
  mkdir -- "$work" # Refuse to reuse a partial or completed preparation.
fi
work=$(cd -- "$work" && pwd)
cd -- "$work"
trap 'echo "Migration stopped. Files retained in $work; inspect the reports. No automatic cleanup or rollback was performed." >&2' ERR

if [[ $action == prepare ]]; then
  "$python" "$helper" snapshot "$work" > snapshot-report.json
  mkdir v3
  args=(--persistence sqlite --data-dir "$work/v3" migrate --from-v2 "$work/v2.db")
  if [[ -n ${GPROXY_MASTER_KEY:-} ]]; then
    # v3.0.22 exposes the source key only as a CLI option, not an env variable.
    args+=(--from-v2-master-key "$GPROXY_MASTER_KEY")
  fi
  run_logged v3-preflight.log "${args[@]}"
  run_logged v3-import.log "${args[@]}" --apply
  "$python" "$helper" verify-v3 "$work" > v3-verification.json
  touch prepared
  echo "Prepared $work/v3/gproxy.db. Review snapshot-report.json, v3-import.log and v3-verification.json."
  echo 'Then run the import command with a new, empty PostgreSQL database.'
else
  : "${GPROXY_V4_TARGET_DSN:?Set GPROXY_V4_TARGET_DSN}"
  [[ -f prepared ]] || { echo 'No successful preparation in this directory' >&2; exit 1; }
  "$python" "$helper" check-target "$work"
  mkdir v4
  migration_dsn=$GPROXY_V4_TARGET_DSN
  run_logged v4-import.log --persistence postgres --data-dir "$work/v4" \
    import --from-v3 "$work/v3/gproxy.db"
  "$python" "$helper" verify-v4 "$work" > v4-verification.json
  touch imported
  echo "Import verified. Review $work/v4-import.log and v4-verification.json before switching your deployment."
fi
