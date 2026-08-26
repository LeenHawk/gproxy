#!/usr/bin/env bash
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
output_dir="${OUTPUT_DIR:-$PWD/dist/release}"
binary="target/$target/release/gproxy"
version="$(scripts/release-metadata.sh version)"

case "$target" in
  x86_64-*) architecture=amd64 ;;
  aarch64-*) architecture=arm64 ;;
  riscv64gc-*) architecture=riscv64 ;;
  *) echo "unsupported deb architecture: $target" >&2; exit 1 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
root="$work/root"
mkdir -p "$root/DEBIAN" "$root/usr/bin" "$root/usr/share/applications" \
  "$root/usr/share/doc/gproxy" "$root/etc/xdg/autostart" "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"

install -m 0755 "$binary" "$root/usr/bin/gproxy"
install -m 0755 scripts/installers/linux/gproxy-desktop "$root/usr/bin/gproxy-desktop"
install -m 0644 scripts/installers/linux/gproxy.desktop "$root/usr/share/applications/gproxy.desktop"
install -m 0644 scripts/installers/linux/gproxy-autostart.desktop "$root/etc/xdg/autostart/gproxy.desktop"
install -m 0644 README.md "$root/usr/share/doc/gproxy/README.md"
install -m 0644 LICENSE "$root/usr/share/doc/gproxy/copyright"

sed -e "s/__VERSION__/$version/" -e "s/__ARCHITECTURE__/$architecture/" \
  scripts/installers/linux/control.in > "$root/DEBIAN/control"
find "$root" -type d -exec chmod 0755 {} +
dpkg-deb --build --root-owner-group "$root" "$output_dir/$artifact.deb" >/dev/null
(cd "$output_dir" && sha256sum "$artifact.deb" > "$artifact.deb.sha256")
