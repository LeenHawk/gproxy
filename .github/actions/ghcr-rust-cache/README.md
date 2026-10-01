# GHCR Rust build cache

Release and CI keep Rust caches in `ghcr.io/<owner>/<repo>-build-cache`.
These OCI artifacts use GHCR storage, not the repository's Actions cache quota.
GitHub currently provides Container registry storage and bandwidth for free:
https://docs.github.com/en/billing/concepts/product-billing/github-packages#free-use-of-github-packages

The action restores `target/`, Cargo's registry and Git dependencies before the
build, then uploads them in a successful job's post step. Incremental state is
excluded. Cargo/rustup executables remain managed by the toolchain setup steps.
Musl's nested Cargo and Go caches are included through `target/`.
Archives stream through multithreaded zstd at level 3. This changes only cache
transport; release LTO, binary optimization and UPX compression are unaffected.
The action retries a failed ORAS installation once.
The cache also records hashes and timestamps of tracked sources and generated
Console assets. Unchanged files regain their previous timestamps after checkout,
allowing Cargo to reuse workspace crates as well as dependencies. Changed files
get fresh timestamps and are rebuilt; their contents are never overwritten.
PAX archives retain subsecond build timestamps used by Cargo's fingerprints.

Each OS, runner architecture and caller-provided build key has one rolling tag.
Cargo fingerprints handle changes in compiler, build flags and dependencies.
The zstd archives use `v2-` tags. A missing v2 cache falls back to the existing
v1 gzip archive, so the migration does not require a cold build. `restore-key`
can also seed a newly split job from an older compatible cache. Only the job's
own v2 tag is updated; old workflow runs can still read their v1 archives.
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

When neither a current nor a compatible older cache exists, the job starts cold
and populates GHCR; existing Actions caches are not copied or deleted. Missing
caches, ORAS installation failures and registry errors do not fail the build.
Full upload/download and Windows/macOS runner behavior must be verified by
GitHub Actions after pushing the change.
