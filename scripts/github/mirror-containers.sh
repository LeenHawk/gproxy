#!/usr/bin/env bash
# Copy the complete OCI indexes, including attestations, without rebuilding.
set -euo pipefail
printf '%s' "$GH_TOKEN" | skopeo login ghcr.io --username LeenHawk --password-stdin
printf '%s' "$GITLAB_RELEASE_TOKEN" | skopeo login registry.gitlab.com --username oauth2 --password-stdin
printf '%s' "$CNB_TOKEN" | skopeo login docker.cnb.cool --username cnb --password-stdin
tags=("$RELEASE_TAG")
if [ "$GPROXY_BUILD_CHANNEL" = release ]; then tags+=(staging); fi
for suffix in "" -musl; do
  for destination in ghcr.io/leenhawk/gproxy "$CI_REGISTRY_IMAGE" docker.cnb.cool/leenhawk/gproxy; do
    for tag in "${tags[@]}"; do
      skopeo copy --all --preserve-digests \
        "docker://ghcr.io/leenhawk/gproxy:$CI_COMMIT_SHA$suffix" \
        "docker://$destination:$tag$suffix"
    done
  done
done
