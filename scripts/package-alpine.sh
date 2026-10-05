#!/usr/bin/env bash
# Run inside the Alpine builder; apk-tools 3 creates native APKv3 packages.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/reproducible-env.sh"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
: "${TARGET_TRIPLE:?}" "${ARTIFACT_NAME:?}" "${GPROXY_BUILD_VERSION:?}"
mode="${1:?application|cli|headless}"
case "$TARGET_TRIPLE" in
  x86_64-unknown-linux-musl) arch=x86_64 ;;
  aarch64-unknown-linux-musl) arch=aarch64 ;;
  riscv64gc-unknown-linux-musl) arch=riscv64 ;;
  *) echo "unsupported Alpine target: $TARGET_TRIPLE" >&2; exit 1 ;;
esac
case "$mode" in
  application) package=gproxy-desktop; executable=gproxy-desktop; description='GPROXY desktop application' ;;
  cli) package=gproxy-cli; executable=gproxy; description='GPROXY command-line server' ;;
  headless) package=gproxy-headless; executable=gproxy; description='GPROXY command-line server without the console' ;;
  *) echo "unsupported Alpine package: $mode" >&2; exit 1 ;;
esac
binary="${ALPINE_BINARY:-$root/target/$TARGET_TRIPLE/release/$executable}"
output="${OUTPUT_DIR:-$root/dist/release}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
payload="$work/package"
mkdir -p "$output"
install -Dm755 "$binary" "$payload/usr/bin/$executable"
install -Dm644 LICENSE "$payload/usr/share/licenses/$package/LICENSE"
install -Dm644 distribution/alpine/README.txt "$payload/usr/share/doc/$package/README.txt"
dependencies=
if [ "$mode" = application ]; then
  install -Dm644 crates/gproxy-host-tauri/icons/icon.png "$payload/usr/share/icons/hicolor/512x512/apps/gproxy.png"
  install -Dm644 distribution/alpine/gproxy.desktop "$payload/usr/share/applications/gproxy.desktop"
  # Include ELF SONAME requirements, plus the tray library loaded with dlopen.
  dependencies="$(scanelf --needed --nobanner --format '%n#F' "$binary" | tr ',' '\n' | sort -u | sed 's/^/so:/' | tr '\n' ' ')libayatana-appindicator librsvg"
  upx_args=(--best --lzma)
  [ "$arch" != riscv64 ] || upx_args+=(--no-filter)
  upx "${upx_args[@]}" "$payload/usr/bin/$executable"
  upx --test "$payload/usr/bin/$executable"
elif [ "$mode" = cli ]; then
  dependencies='!gproxy-headless'
else
  dependencies='!gproxy-cli'
fi
version="${GPROXY_BUILD_VERSION/-beta./_beta}"
version="${version/-dev./_alpha}"
version="${version/-rc./_rc}"
apk version --check "$version-r0"
# Only the public key belongs in source control or public packages.
sign_args=()
if [ "${GPROXY_UNSIGNED_BUILD:-0}" != 1 ]; then
  umask 077
  if [ -n "${ALPINE_SIGNING_PRIVATE_KEY_B64:-}" ]; then
    printf '%s' "$ALPINE_SIGNING_PRIVATE_KEY_B64" | base64 -d > "$work/signing.rsa"
  else
    cp "${ALPINE_SIGNING_KEY_FILE:-$root/dev_docs/alpine-signing/gproxy-alpine.rsa}" "$work/signing.rsa"
  fi
  openssl pkey -in "$work/signing.rsa" -pubout -out "$work/signing.rsa.pub"
  cmp "$work/signing.rsa.pub" distribution/alpine/gproxy-alpine.rsa.pub
  sign_args=(--sign-key "$work/signing.rsa")
fi
python3 scripts/reproducible-env.py --normalize-tree "$payload"
fakeroot bash -c 'chown -R 0:0 "$1"; shift; exec apk mkpkg "$@"' sh "$payload" --files "$payload" --output "$output/$ARTIFACT_NAME.apk" \
  "${sign_args[@]}" \
  --info "name:$package" --info "version:$version-r0" \
  --info "arch:$arch" --info 'license:AGPL-3.0-or-later' \
  --info "description:$description" \
  --info 'url:https://github.com/LeenHawk/gproxy' \
  --info "depends:$dependencies"
chmod 644 "$output/$ARTIFACT_NAME.apk"
if [ "${GPROXY_UNSIGNED_BUILD:-0}" = 1 ]; then
  apk --allow-untrusted verify "$output/$ARTIFACT_NAME.apk"
else
  apk --keys-dir "$root/distribution/alpine" verify "$output/$ARTIFACT_NAME.apk"
fi
(cd "$output" && sha256sum "$ARTIFACT_NAME.apk" > "$ARTIFACT_NAME.apk.sha256")
chmod 644 "$output/$ARTIFACT_NAME.apk.sha256"
if [ "$mode" = application ]; then
  cp distribution/alpine/README.txt "$payload/RUN.txt"
  cp distribution/alpine/gproxy-alpine.rsa.pub "$payload/"
  rm -f "$output/$ARTIFACT_NAME.zip"
  python3 scripts/reproducible-archive.py --root "$payload" --output "$output/$ARTIFACT_NAME.zip"
  chmod 644 "$output/$ARTIFACT_NAME.zip"
  bash scripts/package-musl-application-deb.sh "$output/$ARTIFACT_NAME.apk"
fi
