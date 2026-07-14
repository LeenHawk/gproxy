#!/usr/bin/env bash
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
binary="target/$target/release/gproxy"
version="$(awk '/^version = / { gsub(/\"/, "", $3); print $3; exit }' Cargo.toml)"

case "$target" in
  x86_64-*) deb_arch=amd64 ;;
  aarch64-*) deb_arch=arm64 ;;
  riscv64gc-*) deb_arch=riscv64 ;;
  *) echo "unsupported deb architecture: $target" >&2; exit 1 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
root="$work/root"
mkdir -p \
  "$root/DEBIAN" \
  "$root/usr/bin" \
  "$root/usr/share/applications" \
  "$root/usr/share/doc/gproxy" \
  "$root/usr/share/icons/hicolor/96x96/apps" \
  "$root/etc/xdg/autostart"

install -m 0755 "$binary" "$root/usr/bin/gproxy"
install -m 0755 scripts/installers/linux/gproxy-desktop "$root/usr/bin/gproxy-desktop"
install -m 0644 scripts/installers/linux/gproxy.desktop \
  "$root/usr/share/applications/gproxy.desktop"
install -m 0644 scripts/installers/linux/gproxy-autostart.desktop \
  "$root/etc/xdg/autostart/gproxy.desktop"
install -m 0644 console/public/favicon-96x96.png \
  "$root/usr/share/icons/hicolor/96x96/apps/gproxy.png"
install -m 0644 README.md "$root/usr/share/doc/gproxy/README.md"
install -m 0644 LICENSE "$root/usr/share/doc/gproxy/copyright"

cat > "$root/DEBIAN/control" <<EOF
Package: gproxy
Version: $version
Section: net
Priority: optional
Architecture: $deb_arch
Maintainer: GPROXY maintainers
Description: High-performance LLM proxy with an embedded management console
 Installs a desktop launcher and starts GPROXY in the background at user login.
EOF

find "$root" -type d -exec chmod 0755 {} +
dpkg-deb --build --root-owner-group "$root" "$artifact.deb" >/dev/null
sha256sum "$artifact.deb" > "$artifact.deb.sha256"
