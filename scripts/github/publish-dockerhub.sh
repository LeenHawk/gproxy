#!/usr/bin/env bash
# Mirror the release OCI indexes, including every architecture and attestation.
set -euo pipefail
eval "$(python3 scripts/release-context.py --shell)"
if [ -n "$GPROXY_PUBLISH_BRANCH" ]; then
  latest="$(git ls-remote "https://github.com/$GITHUB_REPOSITORY.git" "refs/heads/$GPROXY_PUBLISH_BRANCH" | cut -f1)"
  if [ "$latest" != "$GITHUB_SHA" ]; then
    echo "Skipping stale build: $GPROXY_PUBLISH_BRANCH has advanced."
    exit 0
  fi
fi
printf '%s' "$GH_TOKEN" | skopeo login ghcr.io --username "$GITHUB_REPOSITORY_OWNER" --password-stdin
printf '%s' "$DOCKERHUB_TOKEN" | skopeo login docker.io --username leenhawk --password-stdin

tags=("$RELEASE_TAG")
if [ "$GPROXY_BUILD_CHANNEL" = release ]; then tags+=(staging); fi
source="ghcr.io/${GITHUB_REPOSITORY,,}:$GITHUB_SHA-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"
for suffix in "" -musl; do
  for tag in "${tags[@]}"; do
    skopeo copy --all --preserve-digests \
      "docker://$source$suffix" \
      "docker://docker.io/leenhawk/gproxy:$tag$suffix"
  done
done
