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
      pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked
      binary="$root/target/$TARGET_TRIPLE/release/gproxy-desktop"
      upx --best --lzma "$binary"
      upx --test "$binary"
      pnpm exec tauri bundle --target "$TARGET_TRIPLE" --bundles "$bundle" --config "$config" --no-binary-patching
    else
      pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --bundles "$bundle" --config "$config" -- --locked
    fi
    files=("$root/target/$TARGET_TRIPLE/release/bundle/$bundle/"*."$bundle")
    test "${#files[@]}" -eq 1 && test -f "${files[0]}"
    cp "${files[0]}" "$output/$ARTIFACT_NAME.$bundle"
    ;;
  windows)
    (
      # Android needs the cdylib entry point; the desktop executable only uses
      # the Rust library. Avoid linking an unused DLL before linking the EXE.
      manifest_backup="$(mktemp)"
      cp Cargo.toml "$manifest_backup"
      trap 'cp "$manifest_backup" Cargo.toml; rm -f "$manifest_backup"' EXIT
      node -e '
        const fs = require("node:fs");
        const text = fs.readFileSync("Cargo.toml", "utf8");
        const types = "crate-type = [\"lib\", \"cdylib\"]";
        if (!text.includes(types)) throw new Error("Unexpected Tauri library crate types");
        fs.writeFileSync("Cargo.toml", text.replace(types, "crate-type = [\"lib\"]"));
      '
      pnpm exec tauri build --ci --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked
    )
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
    pnpm exec tauri android build --ci --apk --target "$arch" --config "$config" -- --locked
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
    python3 scripts/pack-android-application.py "${files[0]}" "$work/packed.apk"
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
