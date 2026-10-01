#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test "$(git branch --show-current)" = dev || { echo 'Run this from dev.' >&2; exit 1; }
git diff --quiet && git diff --cached --quiet || { echo 'Commit your changes before rebasing dev.' >&2; exit 1; }
git fetch origin main dev
previous="$(git rev-parse refs/remotes/origin/dev)"
git rebase origin/main
git merge-base --is-ancestor origin/main HEAD
git push --force-with-lease="refs/heads/dev:$previous" origin HEAD:refs/heads/dev
