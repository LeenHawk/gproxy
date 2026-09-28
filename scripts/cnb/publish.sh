#!/usr/bin/env bash
set -euo pipefail
source scripts/cnb/env.sh
if [ "$GPROXY_BUILD_CHANNEL" = dev ]; then
  latest="$(git ls-remote origin refs/heads/dev | cut -f1)"
  if [ "$latest" != "$CNB_COMMIT" ]; then
    echo "Skipping stale nightly publication: dev has advanced."
    exit 0
  fi
fi
registry="${CNB_DOCKER_REGISTRY:-docker.cnb.cool}"
image="$registry/${CNB_REPO_SLUG,,}"
printf '%s' "$CNB_TOKEN" | docker login "$registry" --username cnb --password-stdin
context="$RUNNER_TEMP/container"
mkdir -p "$context/dist/data"
for variant in gnu musl; do
  suffix=
  base=gcr.io/distroless/cc-debian13
  if [ "$variant" = musl ]; then suffix=-musl; base=gcr.io/distroless/static-debian13; fi
  for arch in amd64 arm64; do
    rust_arch=x86_64
    [ "$arch" != arm64 ] || rust_arch=aarch64
    cp "dist/container/$rust_arch-unknown-linux-$variant/gproxy" "$context/dist/gproxy"
    docker buildx build --platform "linux/$arch" --push \
      --file deploy/container/Dockerfile.release --build-arg "RUNTIME_BASE=$base" \
      --label "org.opencontainers.image.revision=$CNB_COMMIT" \
      --label "org.opencontainers.image.source=https://cnb.cool/$CNB_REPO_SLUG" \
      --tag "$image:$CNB_COMMIT-$arch$suffix" "$context"
  done
  docker buildx imagetools create --tag "$image:$RELEASE_TAG$suffix" \
    "$image:$CNB_COMMIT-amd64$suffix" "$image:$CNB_COMMIT-arm64$suffix"
done
python3 scripts/cnb/api.py publish
