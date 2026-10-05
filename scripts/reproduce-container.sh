#!/usr/bin/env bash
# Compare runnable OCI images separately from registry attestations/signatures.
set -euo pipefail
target="${1:?target}"
input="${2:?downloaded native comparison artifact}"
case "$target" in
  x86_64-*) arch=amd64 ;;
  aarch64-*) arch=arm64 ;;
  riscv64gc-*) arch=riscv64 ;;
  *) exit 2 ;;
esac
case "$target" in
  *-gnu) variant=gnu; base=gcr.io/distroless/cc-debian13 ;;
  *-musl) variant=musl; base=gcr.io/distroless/static-debian13 ;;
  *) exit 2 ;;
esac
source scripts/reproducible-env.sh
python3 - "$input/report.json" "$GPROXY_BUILD_HASH" <<'PY'
import json, sys
report = json.load(open(sys.argv[1]))
assert report['commit'] == sys.argv[2] and report['passed'], 'Native comparison did not pass for this source'
PY
docker pull --platform "linux/$arch" "$base"
resolved="$(docker image inspect "$base" --format '{{index .RepoDigests 0}}')"
output="$PWD/dist/reproducible-container"
context="$RUNNER_TEMP/gproxy-oci-context"
mkdir -p "$context/dist/data" "$output"
cp deploy/container/Dockerfile.release "$context/Dockerfile"
printf '%s\n' "$resolved" > "$output/runtime-base.txt"
for attempt in first second; do
  cp "$input/$attempt/container-input/$target/gproxy" "$context/dist/gproxy"
  mkdir -p "$output/$attempt"
  docker buildx build --no-cache --platform "linux/$arch" \
    --build-arg "RUNTIME_BASE=$resolved" --build-arg "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH" \
    --provenance=false --sbom=false --tag "gproxy-reproduce:$variant-$arch" \
    --output "type=oci,dest=$output/$attempt/gproxy-container-$variant-$arch.oci.tar,rewrite-timestamp=true" \
    "$context"
done
python3 - "$output" "$target" "$GPROXY_BUILD_HASH" "$resolved" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
files = []
for first in (root / 'first').iterdir():
    second = root / 'second' / first.name
    with first.open('rb') as stream:
        left = hashlib.file_digest(stream, 'sha256').hexdigest()
    with second.open('rb') as stream:
        right = hashlib.file_digest(stream, 'sha256').hexdigest()
    files.append(dict(name=first.name, first=left, second=right, equal=left == right))
passed = bool(files) and all(f['equal'] for f in files)
report = dict(commit=sys.argv[3], target=sys.argv[2], runtime_base=sys.argv[4], passed=passed, files=files)
(root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
sys.exit(0 if passed else 1)
PY
