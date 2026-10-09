#!/usr/bin/env bash
# Build the application host, keeping its identity separate from server archives.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/reproducible-env.sh"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
: "${TARGET_TRIPLE:?}"
: "${TARGET_OS:?}"
: "${ARTIFACT_NAME:?}"
: "${GPROXY_BUILD_VERSION:?}"
output="$root/dist/release"
mkdir -p "$output"
config="$(jq -cn --arg version "$GPROXY_BUILD_VERSION" '{version:$version,bundle:{active:true}}')"
cd crates/gproxy-host-tauri
case "$TARGET_OS:$TARGET_TRIPLE" in
  linux:*-musl)
    bash "$root/scripts/build-musl-application.sh"
    ;;
  linux:* | macos:*)
    bundle=deb
    [ "$TARGET_OS" != macos ] || bundle=dmg
    if [ "$TARGET_OS" = linux ]; then
      bash "$root/scripts/with-tauri-desktop-lib.sh" pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked
      binary="$root/target/$TARGET_TRIPLE/release/gproxy-desktop"
      upx_args=(--best --lzma)
      if [[ "$TARGET_TRIPLE" == riscv64gc-* ]]; then upx_args+=(--no-filter); fi
      upx "${upx_args[@]}" "$binary"
      upx --test "$binary"
      pnpm exec tauri bundle --target "$TARGET_TRIPLE" --bundles "$bundle" --config "$config" --no-binary-patching
    else
      # Keep the .app as a requested output; DMG-only bundling deletes it before ZIP packaging.
      bash "$root/scripts/with-tauri-desktop-lib.sh" pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --bundles app --config "$config" -- --locked
    fi
    if [ "$TARGET_OS" = macos ]; then
      apps=("$root/target/$TARGET_TRIPLE/release/bundle/macos/"*.app)
      test "${#apps[@]}" -eq 1 && test -d "${apps[0]}"
      work="$(mktemp -d)"
      trap 'rm -rf "$work"' EXIT
      cp -R "${apps[0]}" "$work/"
      ln -s /Applications "$work/Applications"
      python3 "$root/scripts/reproducible-dmg.py" --source "$work" --output "$output/$ARTIFACT_NAME.dmg" --volume-name GPROXY
      python3 "$root/scripts/reproducible-archive.py" --root "$(dirname "${apps[0]}")" --output "$output/$ARTIFACT_NAME.zip" "$(basename "${apps[0]}")"
      rm -rf "$work"
      trap - EXIT
    else
      files=("$root/target/$TARGET_TRIPLE/release/bundle/deb/"*.deb)
      test "${#files[@]}" -eq 1 && test -f "${files[0]}"
      python3 "$root/scripts/reproducible-deb.py" "${files[0]}" "$output/$ARTIFACT_NAME.deb"
      work="$(mktemp -d)"
      dpkg-deb -x "${files[0]}" "$work"
      cp "$root/README.md" "$root/LICENSE" "$work/"
      printf '%s\n' 'Run ./usr/bin/gproxy-desktop. GTK 3 and WebKitGTK 4.1 runtime libraries are required; the DEB installs those dependencies automatically.' > "$work/RUN.txt"
      python3 "$root/scripts/reproducible-archive.py" --root "$work" --output "$output/$ARTIFACT_NAME.zip"
      rm -rf "$work"
    fi
    ;;
  windows:*)
    bash "$root/scripts/with-tauri-desktop-lib.sh" pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked
    binary="$root/target/$TARGET_TRIPLE/release/gproxy-desktop.exe"
    upx --best --lzma "$binary"
    upx --test "$binary"
    cd "$root"
    pwsh -NoProfile -File scripts/package-windows-msix.ps1 \
      -Target "$TARGET_TRIPLE" -Artifact "$ARTIFACT_NAME" -Version "$GPROXY_BUILD_VERSION" \
      -OutputDir dist/release
    pwsh -NoProfile -File scripts/package-application-zip.ps1 \
      -Target "$TARGET_TRIPLE" -Artifact "$ARTIFACT_NAME" -OutputDir dist/release
    ;;
  android:*)
    cd "$root"
    # Reuse the channel-aware Android builder; this entrypoint owns release signing.
    GPROXY_ANDROID_FRONTEND_READY=1 bash scripts/mobile/build-android.sh direct "${TARGET_TRIPLE%%-*}"
    source scripts/android/sdk.sh
    sdk="$(android_sdk_root)"
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    cp "dist/mobile/direct/${TARGET_TRIPLE%%-*}/gproxy-direct-${TARGET_TRIPLE%%-*}.apk" "$work/aligned.apk"
    if [ "${GPROXY_UNSIGNED_BUILD:-0}" = 1 ]; then
      cp "$work/aligned.apk" "$output/$ARTIFACT_NAME.apk"
    else
    : "${ANDROID_SIGNING_KEYSTORE_B64:?}"
    : "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?}"
    : "${ANDROID_SIGNING_KEY_ALIAS:?}"
    printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$work/key.jks"
    chmod 600 "$work/key.jks"
    signer_args=(--ks "$work/key.jks" --ks-pass env:ANDROID_SIGNING_KEYSTORE_PASSWORD --ks-key-alias "$ANDROID_SIGNING_KEY_ALIAS" --v4-signing-enabled false)
    if [ -n "${ANDROID_SIGNING_KEY_PASSWORD:-}" ]; then signer_args+=(--key-pass env:ANDROID_SIGNING_KEY_PASSWORD); fi
    signer="$(android_build_tool "$sdk" apksigner)"
    "$signer" sign "${signer_args[@]}" --out "$output/$ARTIFACT_NAME.apk" "$work/aligned.apk"
    "$signer" verify --verbose "$output/$ARTIFACT_NAME.apk"
    fi
    ;;
  *) echo "unsupported application OS: $TARGET_OS" >&2; exit 1 ;;
esac
cd "$output"
for file in "$ARTIFACT_NAME".*; do
  [ "${file##*.}" != sha256 ] || continue
  if command -v sha256sum >/dev/null; then sha256sum "$file"; else shasum -a 256 "$file"; fi > "$file.sha256"
done
