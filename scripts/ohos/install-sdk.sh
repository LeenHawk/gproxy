#!/usr/bin/env bash
set -euo pipefail
: "${OHOS_HOME:?set the destination SDK directory}"
if [ -x "$OHOS_HOME/native/llvm/bin/clang" ]; then exit 0; fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
url=https://repo.huaweicloud.com/openharmony/os/6.0.0.1-Release/ohos-sdk-windows_linux-public.tar.gz
curl --fail --location --retry 3 "$url" -o "$work/sdk.tar.gz"
printf '%s  %s\n' 42955c8106cab9349a5eabfdc055543b9e49111a4ea68705f7a54bdf65c7834a "$work/sdk.tar.gz" | sha256sum -c -
tar -xzf "$work/sdk.tar.gz" -C "$work"
mkdir -p "$OHOS_HOME"
while IFS= read -r package; do
  echo "Installing $(basename "$package")"
  unzip -q -o "$package" -d "$OHOS_HOME"
done < <(find "$work" -type f -name '*linux*.zip' | sort)
test -x "$OHOS_HOME/native/llvm/bin/clang"
"$OHOS_HOME/native/llvm/bin/clang" --version
