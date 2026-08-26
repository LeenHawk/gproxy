#!/usr/bin/env bash
set -euo pipefail

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
artifact="${ARTIFACT_NAME:?missing ARTIFACT_NAME}"
output_dir="${OUTPUT_DIR:-$PWD/dist/release}"
package_name="${ANDROID_PACKAGE_NAME:-io.github.leenhawk.gproxy}"
version_name="$(scripts/release-metadata.sh version)"
binary="target/$target/release/gproxy"
source scripts/android/sdk.sh

if [ -z "${ANDROID_SIGNING_KEYSTORE_B64:-}" ] && [ -z "${ANDROID_SIGNING_KEYSTORE:-}" ]; then
  echo "missing Android release signing key; set ANDROID_SIGNING_KEYSTORE_B64 or ANDROID_SIGNING_KEYSTORE" >&2
  exit 1
fi
: "${ANDROID_SIGNING_KEYSTORE_PASSWORD:?missing ANDROID_SIGNING_KEYSTORE_PASSWORD}"
: "${ANDROID_SIGNING_KEY_ALIAS:?missing ANDROID_SIGNING_KEY_ALIAS}"
if [ -n "${ANDROID_SIGNING_KEYSTORE:-}" ] && [ ! -f "$ANDROID_SIGNING_KEYSTORE" ]; then
  echo "missing Android signing keystore: $ANDROID_SIGNING_KEYSTORE" >&2
  exit 1
fi

if [[ ! "$package_name" =~ ^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$ ]]; then
  echo "invalid Android package name: $package_name" >&2
  exit 1
fi
if [[ "$version_name" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
  version_code="$((10#${BASH_REMATCH[1]} * 1000000 + 10#${BASH_REMATCH[2]} * 1000 + 10#${BASH_REMATCH[3]}))"
else
  echo "workspace version cannot produce an Android version code" >&2
  exit 1
fi

sdk="$(android_sdk_root)"
android_jar="$(android_platform_jar "$sdk")"
aapt="$(android_build_tool "$sdk" aapt)"
zipalign="$(android_build_tool "$sdk" zipalign)"
apksigner="$(android_build_tool "$sdk" apksigner)"
d8="$(android_d8 "$sdk")"
abi="$(android_abi "$target")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
source_dir="$work/src/${package_name//.//}"
mkdir -p "$source_dir" "$work/classes" "$work/dex" \
  "$work/assets/gproxy" "$work/native/lib/$abi" "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"

sources=()
for template in scripts/android/*.java.in; do
  output="$source_dir/$(basename "${template%.in}")"
  sed "s/__PACKAGE__/$package_name/g" "$template" > "$output"
  sources+=("$output")
done
javac -source 8 -target 8 -bootclasspath "$android_jar" -d "$work/classes" "${sources[@]}"
mapfile -t classes < <(find "$work/classes" -name '*.class' -type f | sort)
"$d8" --min-api 28 --lib "$android_jar" --output "$work/dex" "${classes[@]}"

install -m 0755 "$binary" "$work/assets/gproxy/gproxy.bin"
install -m 0644 "$(android_libcxx "$target")" "$work/assets/gproxy/libc++_shared.so"
install -m 0644 "$(android_libcxx "$target")" "$work/native/lib/$abi/libc++_shared.so"
install -m 0644 README.md LICENSE "$work/assets/gproxy/"

sed -e "s/__PACKAGE__/$package_name/g" \
  -e "s/__VERSION_CODE__/$version_code/g" \
  -e "s/__VERSION_NAME__/$version_name/g" \
  scripts/android/AndroidManifest.xml.in > "$work/AndroidManifest.xml"
"$aapt" package -f -M "$work/AndroidManifest.xml" -I "$android_jar" \
  -S scripts/android/res \
  -A "$work/assets" -F "$work/base.apk" >/dev/null
(cd "$work/dex" && zip -q "$work/base.apk" classes.dex)
(cd "$work/native" && zip -q -r "$work/base.apk" lib)
unsigned="$output_dir/$artifact.unsigned.apk"
"$zipalign" -f -p 4 "$work/base.apk" "$unsigned"

keystore="${ANDROID_SIGNING_KEYSTORE:-}"
if [ -n "${ANDROID_SIGNING_KEYSTORE_B64:-}" ]; then
  keystore="$work/release.keystore"
  printf '%s' "$ANDROID_SIGNING_KEYSTORE_B64" | base64 -d > "$keystore"
  chmod 600 "$keystore"
fi
signer_args=(
  --v4-signing-enabled false
  --ks "$keystore"
  --ks-key-alias "$ANDROID_SIGNING_KEY_ALIAS"
  --ks-pass env:ANDROID_SIGNING_KEYSTORE_PASSWORD
)
if [ -n "${ANDROID_SIGNING_KEY_PASSWORD:-}" ]; then
  signer_args+=(--key-pass env:ANDROID_SIGNING_KEY_PASSWORD)
fi
apk="$output_dir/$artifact.apk"
"$apksigner" sign "${signer_args[@]}" --out "$apk" "$unsigned"
"$apksigner" verify --verbose "$apk" >/dev/null
rm -f "$unsigned" "$unsigned.sha256"
(cd "$output_dir" && sha256sum "$artifact.apk" > "$artifact.apk.sha256")
