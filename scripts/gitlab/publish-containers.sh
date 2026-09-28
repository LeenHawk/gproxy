#!/usr/bin/env bash
set -euo pipefail
source scripts/gitlab/env.sh
printf '%s' "$GH_TOKEN" | docker login ghcr.io --username LeenHawk --password-stdin
printf '%s' "$CI_REGISTRY_PASSWORD" | docker login "$CI_REGISTRY" --username "$CI_REGISTRY_USER" --password-stdin
printf '%s' "$CNB_TOKEN" | docker login docker.cnb.cool --username cnb --password-stdin
images=(ghcr.io/leenhawk/gproxy "$CI_REGISTRY_IMAGE" docker.cnb.cool/leenhawk/gproxy)
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
    tags=()
    for image in "${images[@]}"; do tags+=(--tag "$image:$CI_COMMIT_SHA-$arch$suffix"); done
    docker buildx build --platform "linux/$arch" --push --file deploy/container/Dockerfile.release \
      --build-arg "RUNTIME_BASE=$base" --label "org.opencontainers.image.revision=$CI_COMMIT_SHA" \
      --label "org.opencontainers.image.source=https://github.com/LeenHawk/gproxy" "${tags[@]}" "$context"
  done
  for image in "${images[@]}"; do
    docker buildx imagetools create --tag "$image:$RELEASE_TAG$suffix" \
      "$image:$CI_COMMIT_SHA-amd64$suffix" "$image:$CI_COMMIT_SHA-arm64$suffix"
  done
done
