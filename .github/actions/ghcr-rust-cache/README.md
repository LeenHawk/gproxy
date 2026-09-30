# GHCR Rust build cache

Release and CI keep Rust caches in `ghcr.io/<owner>/<repo>-build-cache`.
These OCI artifacts use GHCR storage, not the repository's Actions cache quota.
GitHub currently provides Container registry storage and bandwidth for free:
https://docs.github.com/en/billing/concepts/product-billing/github-packages#free-use-of-github-packages

The action restores `target/`, Cargo's registry and Git dependencies before the
build, then uploads them in a successful job's post step. Incremental state is
excluded. Cargo/rustup executables remain managed by the toolchain setup steps.
Musl's nested Cargo and Go caches are included through `target/`.

Each OS, runner architecture and caller-provided build key has one rolling tag.
Cargo fingerprints handle changes in compiler, build flags and dependencies.
Only `dev` writes; tags, PRs and other branches can restore but cannot overwrite
these shared caches. Forks without package access simply build without a cache.
Do not put credentials or other secrets in the cached directories.

The token needs `packages: read` to restore and `packages: write` to save.
The source annotation links the new package to this repository. If a package
with the same name already exists, grant this repository Actions access in the
package settings. Package visibility is left at GitHub's default; no public
visibility change is required.

`release.yml` cleans untagged versions older than seven days after development
builds, preserving all current platform tags. Cleanup runs as part of Release
because the development branch is not the repository's default branch (scheduled
workflows only run from the default branch). Only this cache package is cleaned.
The package created by this repository's token grants it the admin access needed
for deletion. Cleanup errors are visible but do not block publishing.

The first run starts cold and populates GHCR; existing Actions caches are not
copied or deleted. Missing caches, ORAS installation failures and registry errors
do not fail the build. Full upload/download and Windows/macOS runner behavior
must be verified by GitHub Actions after pushing the change.
