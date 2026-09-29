#!/usr/bin/env bash
# Adapt the shared three-platform publisher to a GitHub Actions build.
set -euo pipefail
export CI_PROJECT_DIR="$GITHUB_WORKSPACE" CI_COMMIT_SHA="$GITHUB_SHA" CI_COMMIT_TAG=
if [ "$GITHUB_REF_TYPE" = tag ]; then export CI_COMMIT_TAG="$GITHUB_REF_NAME"; fi
export CI_PROJECT_ID=86976712 CI_API_V4_URL=https://gitlab.com/api/v4
export CI_PROJECT_URL=https://gitlab.com/leenhawk1/gproxy
export CI_PIPELINE_ID="github-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"
export CI_PIPELINE_URL="$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"
export CI_REGISTRY=registry.gitlab.com CI_REGISTRY_IMAGE=registry.gitlab.com/leenhawk1/gproxy
export CI_REGISTRY_USER=oauth2 CI_REGISTRY_PASSWORD="$GITLAB_RELEASE_TOKEN"
bash scripts/gitlab/publish.sh
