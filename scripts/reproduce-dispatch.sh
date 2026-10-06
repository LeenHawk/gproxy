#!/usr/bin/env bash
set -euo pipefail
target="${1:?target}"
kind="${2:?product}"
if [ -n "${CONTAINER:-}" ]; then
  canonical="$(dirname "$PWD")"
  restore_owner() {
    docker run --rm --volume "$canonical:$canonical" gproxy-reproduce-tools \
      chown -R --no-dereference "$(id -u):$(id -g)" "$canonical"
  }
  trap restore_owner EXIT
  docker run --rm --volume "$canonical:$canonical" \
    --volume "$REPRO_SOURCE_ROOT:$REPRO_SOURCE_ROOT:ro" --workdir "$PWD" \
    --env CARGO_HOME --env CARGO_TARGET_DIR --env GRADLE_USER_HOME \
    --env SOURCE_DATE_EPOCH --env GPROXY_BUILD_HASH --env GPROXY_UNSIGNED_BUILD \
    --env CARGO_BUILD_JOBS --env CARGO_INCREMENTAL=0 \
    --env GIT_CONFIG_COUNT=1 --env GIT_CONFIG_KEY_0=safe.directory --env "GIT_CONFIG_VALUE_0=$PWD" \
    gproxy-reproduce-tools bash scripts/reproduce-target.sh "$target" "$kind"
  restore_owner
  trap - EXIT
else
  bash scripts/reproduce-target.sh "$target" "$kind"
fi

if [[ "$target" == *-linux-musl && ( "$kind" == cli || "$kind" == headless ) ]]; then
  # Package the same binary with apk-tools, including cross-built RISC-V.
  base="$(awk '/^FROM / { print $2; exit }' deploy/container/Dockerfile.musl)"
  docker build --build-arg "BASE=$base" -t gproxy-reproduce-apk - <<'DOCKERFILE'
ARG BASE
FROM ${BASE}
RUN apk add --no-cache python3 bash fakeroot openssl
DOCKERFILE
  field=artifact
  [ "$kind" != headless ] || field=headless_artifact
  name="$(jq -r --arg target "$target" --arg field "$field" '.include[] | select(.target==$target) | .[$field]' scripts/release-targets.json)"
  version="$(bash scripts/release-metadata.sh version)"
  docker run --rm --volume "$PWD:/workspace" --workdir /workspace \
    --env "TARGET_TRIPLE=$target" --env "ARTIFACT_NAME=$name" --env "GPROXY_BUILD_VERSION=$version" \
    --env SOURCE_DATE_EPOCH --env GPROXY_BUILD_HASH --env GPROXY_UNSIGNED_BUILD \
    gproxy-reproduce-apk bash scripts/package-alpine.sh "$kind"
fi

if [ "$kind" = cli ] && [ -f "dist/container/$target/gproxy" ]; then
  mkdir -p "dist/release/container-input/$target"
  cp "dist/container/$target/gproxy" "dist/release/container-input/$target/gproxy"
fi
