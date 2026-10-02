#!/usr/bin/env bash
# Build the checked-out commit with the same source recipe submitted to Termux.
set -euo pipefail
target="${1:?usage: build-termux.sh TARGET_TRIPLE}"
case "$target" in
  x86_64-linux-android) architecture=x86_64 ;;
  aarch64-linux-android) architecture=aarch64 ;;
  *) echo "unsupported Termux target: $target" >&2; exit 2 ;;
esac
root="$(git rev-parse --show-toplevel)"
cd "$root"
engine="${GPROXY_CONTAINER_ENGINE:-docker}"
pins=distribution/termux/toolchain.json
image="$(jq -r .image "$pins")"
revision="$(jq -r .revision "$pins")"
repository="$(jq -r .repository "$pins")"
export GPROXY_BUILD_VERSION="${GPROXY_BUILD_VERSION:-$(scripts/release-metadata.sh version)}"
export GPROXY_BUILD_CHANNEL="${GPROXY_BUILD_CHANNEL:-release}"
export GPROXY_BUILD_HASH="${GPROXY_BUILD_HASH:-$(git rev-parse HEAD)}"
mkdir -p target
work="$(mktemp -d "$root/target/termux-release.$architecture.XXXXXX")"
container=
cleanup() {
  if [ -n "$container" ]; then "$engine" rm -f "$container" >/dev/null; fi
  rm -rf "$work"
}
trap cleanup EXIT

# Copy through stdin, so this also works with remote Docker daemons in CI.
# Only committed source is a release input; build directories and credentials
# from the runner never enter the builder container.
git archive --format=tar.gz --prefix=gproxy/ HEAD > "$work/source.tar.gz"
container="$("$engine" run -d --device /dev/fuse --cap-add SYS_ADMIN \
  --security-opt seccomp=unconfined --security-opt apparmor=unconfined \
  "$image" sleep infinity)"
"$engine" exec "$container" bash -eu -c '
  cd /home/builder/termux-packages
  git init
  git remote add origin "$1"
  git fetch --depth 1 origin "$2"
  git checkout --detach FETCH_HEAD
' bash "$repository" "$revision"
tar -C distribution/termux -cf - gproxy | "$engine" exec -i "$container" \
  tar -xf - -C /home/builder/termux-packages/packages
"$engine" exec -i "$container" bash -c 'cat > /home/builder/gproxy.tar.gz' < "$work/source.tar.gz"
"$engine" exec \
  --env GPROXY_BUILD_VERSION --env GPROXY_BUILD_CHANNEL --env GPROXY_BUILD_HASH \
  --env GPROXY_BUILD_UPDATE_SOURCE --env GOPROXY --env CARGO_BUILD_JOBS \
  "$container" bash -eu -c '
    cd /home/builder/termux-packages
    # The official recipe pins a published tag. Releases build this checkout,
    # while preserving the recipe, dependency declarations and package hooks.
    printf "\nTERMUX_PKG_VERSION=%q\nTERMUX_PKG_SRCURL=%q\nTERMUX_PKG_SHA256=%q\n" \
      "${GPROXY_BUILD_VERSION//-/\~}" file:///home/builder/gproxy.tar.gz \
      "$(sha256sum /home/builder/gproxy.tar.gz | cut -d " " -f1)" >> packages/gproxy/build.sh
    ./build-package.sh -a "$1" -I -j "${CARGO_BUILD_JOBS:-4}" gproxy
  ' bash "$architecture"

output="target/$target/release"
mkdir -p "$output"
"$engine" exec "$container" bash -eu -c \
  'cat /home/builder/termux-packages/output/gproxy_*_"$1".deb' bash "$architecture" \
  > "$output/termux-package.deb"
dpkg-deb -x "$output/termux-package.deb" "$work/package"
install -m755 "$work/package/data/data/com.termux/files/usr/bin/gproxy" "$output/gproxy"
"$engine" exec "$container" bash -elc '
  node=(/home/builder/.termux-build/_cache/nodejs-*/bin/node)
  pnpm=/home/builder/.termux-build/gproxy/tmp/pnpm/node_modules/pnpm/bin/pnpm.cjs
  jq -n --arg rustc "$(rustc --version)" --arg node "$("${node[0]}" --version)" \
    --arg pnpm "$("${node[0]}" "$pnpm" --version)" \
    "{rustc:\$rustc,node:\$node,pnpm:\$pnpm}"
' > "$output/termux-tools.json"
jq -r .rustc "$output/termux-tools.json" > "$output/rustc-version.txt"
jq -n --arg image "$image" --arg revision "$revision" \
  --arg source_sha256 "$(sha256sum "$work/source.tar.gz" | cut -d ' ' -f1)" \
  --slurpfile tools "$output/termux-tools.json" \
  '{image:$image,revision:$revision,source_sha256:$source_sha256,tools:$tools[0]}' > "$output/termux-toolchain.json"
