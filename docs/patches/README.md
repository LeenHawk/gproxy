# http-cache-semantics 4.2.0

The pnpm patch addresses [GHSA-ch52-4w7c-c8xp](https://github.com/advisories/GHSA-ch52-4w7c-c8xp)
while upstream has no published fixed version. It requires revalidation before
serving responses prohibited from reuse, even when a client requests `max-stale`.
It preserves the library's explicit `public`/`immutable` cookie opt-ins and
ordinary stale caching.

Astro uses this dependency for remote images during the static documentation
build. Its current integration does not call the affected `evaluateRequest` or
`satisfiesWithoutRevalidation` methods. The patch also protects those methods if
future integrations use them.

Remove the patch when upgrading to an upstream release containing the fix.
Version-based dependency audits may continue to report 4.2.0 despite this patch.
