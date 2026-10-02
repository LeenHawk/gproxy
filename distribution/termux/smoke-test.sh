#!/data/data/com.termux/files/usr/bin/bash
set -euo pipefail

: "${PREFIX:?Run this script inside Termux}"
command -v gproxy >/dev/null
command -v curl >/dev/null
port=${GPROXY_TERMUX_TEST_PORT:-18787}
candidate=${1:-}
if [[ -n "$candidate" ]]; then candidate=$(realpath "$candidate"); fi
# Never let an existing instance's DSN or configuration enter this test.
for name in ${!GPROXY_@}; do unset "$name"; done
work=$(mktemp -d)
cd "$work"
pid=
cleanup() {
	if [[ -n "$pid" ]]; then
		kill "$pid" 2>/dev/null || true
		wait "$pid" 2>/dev/null || true
	fi
	rm -rf "$work"
}
trap cleanup EXIT
export GPROXY_DATA_DIR="$work/data"
export GPROXY_MASTER_KEY
GPROXY_MASTER_KEY=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
export GPROXY_BOOTSTRAP_ADMIN_API_KEY="sk-termux-smoke-$GPROXY_MASTER_KEY"
export GPROXY_ADMIN_PASSWORD="$GPROXY_MASTER_KEY"
export GPROXY_ENV_FILE="$work/empty.env"
touch "$GPROXY_ENV_FILE"
base="http://127.0.0.1:$port"

start() {
	gproxy serve --host 127.0.0.1 --port "$port" >"$work/server.log" 2>&1 &
	pid=$!
	for ((attempt = 0; attempt < 120; attempt++)); do
		kill -0 "$pid" 2>/dev/null || { cat "$work/server.log"; return 1; }
		if curl -fsS "$base/healthz" >"$work/health.json" 2>/dev/null; then
			return
		fi
		sleep 1
	done
	echo 'Server readiness timed out' >&2
	return 1
}

old_version=$(gproxy --version)
printf '%s\n' "$old_version"
start
curl -fsS "$base/console/" >"$work/console.html"
grep -q '/console/assets/' "$work/console.html"
curl -fsS -H "Authorization: Bearer $GPROXY_BOOTSTRAP_ADMIN_API_KEY" \
	"$base/admin/api/providers" >"$work/providers.json"
kill "$pid"
wait "$pid"
pid=
if [[ -n "$candidate" ]]; then
	apt install -y "$candidate"
	new_version=$(gproxy --version)
	printf '%s\n' "$new_version"
	[[ "$new_version" != "$old_version" ]]
fi
# The existing key must work after restart without bootstrapping it again.
saved_key=$GPROXY_BOOTSTRAP_ADMIN_API_KEY
unset GPROXY_BOOTSTRAP_ADMIN_API_KEY GPROXY_ADMIN_PASSWORD
start
curl -fsS -H "Authorization: Bearer $saved_key" \
	"$base/admin/api/providers" >"$work/providers-after-restart.json"
cmp "$work/providers.json" "$work/providers-after-restart.json"

executables=("$(command -v gproxy)")
if [[ -f "${executables[0]}.bin" ]]; then executables+=("${executables[0]}.bin"); fi
before=$(sha256sum "${executables[@]}")
for option in --check ''; do
	args=()
	[[ -z "$option" ]] || args+=("$option")
	if gproxy update "${args[@]}" >"$work/update.log" 2>&1; then
		echo "Unexpected successful self-update: $option" >&2
		exit 1
	fi
	grep -q 'pkg upgrade gproxy' "$work/update.log"
done
for action in check apply rollback; do
	code=$(curl -sS -o "$work/update.json" -w '%{http_code}' -X POST \
		-H "Authorization: Bearer $saved_key" "$base/admin/api/update/$action")
	[[ "$code" == 4* ]]
	grep -q 'pkg upgrade gproxy' "$work/update.json"
done
[[ "$before" == "$(sha256sum "${executables[@]}")" ]]
echo 'PASS: startup, Console, admin API, persisted key, package-managed updates'
