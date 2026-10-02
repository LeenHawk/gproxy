#!/usr/bin/env bash
# Build the application host, keeping its identity separate from server archives.
set -euo pipefail
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
case "$TARGET_OS" in
  linux | macos)
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
      bash "$root/scripts/with-tauri-desktop-lib.sh" pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --bundles app,dmg --config "$config" -- --locked
    fi
    files=("$root/target/$TARGET_TRIPLE/release/bundle/$bundle/"*."$bundle")
    test "${#files[@]}" -eq 1 && test -f "${files[0]}"
    cp "${files[0]}" "$output/$ARTIFACT_NAME.$bundle"
    if [ "$TARGET_OS" = macos ]; then
      apps=("$root/target/$TARGET_TRIPLE/release/bundle/macos/"*.app)
      test "${#apps[@]}" -eq 1 && test -d "${apps[0]}"
      ditto -c -k --sequesterRsrc --keepParent "${apps[0]}" "$output/$ARTIFACT_NAME.zip"
    else
      work="$(mktemp -d)"
      # Preserve the DEB payload, including desktop resources, in the ZIP.
      dpkg-deb -x "${files[0]}" "$work"
      cp "$root/README.md" "$root/LICENSE" "$work/"
      printf '%s\n' 'Run ./usr/bin/gproxy-desktop. GTK 3 and WebKitGTK 4.1 runtime libraries are required; the DEB installs those dependencies automatically.' > "$work/RUN.txt"
      (cd "$work" && zip -9 -q -r "$output/$ARTIFACT_NAME.zip" .)
      rm -rf "$work"
    fi
    ;;
  windows)
    bash "$root/scripts/with-tauri-desktop-lib.sh" pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked
    binary="$root/target/$TARGET_TRIPLE/release/gproxy-desktop.exe"
    if [ "$TARGET_TRIPLE" = aarch64-pc-windows-msvc ]; then
      upx --best --nrv2e "$binary"
    else
      upx --best --lzma "$binary"
    fi
    upx --test "$binary"
    cd "$root"
    pwsh -NoProfile -File scripts/package-windows-msix.ps1 \
      -Target "$TARGET_TRIPLE" -Artifact "$ARTIFACT_NAME" -Version "$GPROXY_BUILD_VERSION" \
      -OutputDir dist/release
    pwsh -NoProfile -File scripts/package-application-zip.ps1 \
      -Target "$TARGET_TRIPLE" -Artifact "$ARTIFACT_NAME" -OutputDir dist/release
    ;;
  android)
    source "$root/scripts/android/sdk.sh"
    ndk="$(android_ndk_root)"
    export ANDROID_NDK_HOME="$ndk"
    export GPROXY_ANDROID_ABI="$(android_abi "$TARGET_TRIPLE")"
    export "CMAKE_TOOLCHAIN_FILE_${TARGET_TRIPLE//-/_}=$root/scripts/cmake/android.cmake"
    ranlib=("$ndk"/toolchains/llvm/prebuilt/*/bin/llvm-ranlib)
    test "${#ranlib[@]}" -eq 1 && test -x "${ranlib[0]}"
    # OpenSSL's cc-rs lookup otherwise falls back to the removed GNU ranlib.
    export "RANLIB_${TARGET_TRIPLE//-/_}=${ranlib[0]}"
    arch="${TARGET_TRIPLE%%-*}"
    bindgen="BINDGEN_EXTRA_CLANG_ARGS_${TARGET_TRIPLE//-/_}"
    export "$bindgen=--target=${TARGET_TRIPLE}28"
    bash "$root/scripts/with-tauri-android-lib.sh" pnpm exec tauri android build --ci --apk --target "$arch" --config "$config" -- --locked
    cd "$root"
    sdk="$(android_sdk_root)"
    apk_dir=crates/gproxy-host-tauri/gen/android/app/build/outputs/apk
    mapfile -t files < <(find "$apk_dir" -name '*-release-unsigned.apk')
    test "${#files[@]}" -eq 1
    : "${ANDROID_SIGNING_KEYSTORE_B64:?}"
    : "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?}"
    : "${ANDROID_SIGNING_KEY_ALIAS:?}"
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    strip=("$ndk"/toolchains/llvm/prebuilt/*/bin/llvm-strip)
    python3 scripts/pack-android-application.py "${files[0]}" "$work/packed.apk" "${strip[0]}"
    printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$work/key.jks"
    chmod 600 "$work/key.jks"
    "$(android_build_tool "$sdk" zipalign)" -f -P 16 4 "$work/packed.apk" "$work/aligned.apk"
    signer_args=(--ks "$work/key.jks" --ks-pass env:ANDROID_SIGNING_KEYSTORE_PASSWORD --ks-key-alias "$ANDROID_SIGNING_KEY_ALIAS" --v4-signing-enabled false)
    if [ -n "${ANDROID_SIGNING_KEY_PASSWORD:-}" ]; then signer_args+=(--key-pass env:ANDROID_SIGNING_KEY_PASSWORD); fi
    signer="$(android_build_tool "$sdk" apksigner)"
    "$signer" sign "${signer_args[@]}" --out "$output/$ARTIFACT_NAME.apk" "$work/aligned.apk"
    "$signer" verify --verbose "$output/$ARTIFACT_NAME.apk"
    ;;
  *) echo "unsupported application OS: $TARGET_OS" >&2; exit 1 ;;
esac
cd "$output"
for file in "$ARTIFACT_NAME".*; do
  [ "${file##*.}" != sha256 ] || continue
  if command -v sha256sum >/dev/null; then sha256sum "$file"; else shasum -a 256 "$file"; fi > "$file.sha256"
done
