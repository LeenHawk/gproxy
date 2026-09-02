#!/usr/bin/env bash
# Post-write checks for the docs site: sidebar slugs vs pages, locale parity,
# forbidden references, oversized pages.
set -u
cd "$(dirname "$0")/.." || exit 1
root=docs/src/content/docs
status=0

echo "== sidebar slugs without a page =="
grep -oE "slug: '[a-z0-9/-]+'" docs/astro.config.mjs | sed "s/slug: '//; s/'//" | sort -u | while read -r slug; do
  [ -f "$root/$slug.md" ] || [ -f "$root/$slug.mdx" ] || { echo "  EN missing: $slug"; }
  [ -f "$root/zh-cn/$slug.md" ] || [ -f "$root/zh-cn/$slug.mdx" ] || { echo "  zh-cn missing: $slug"; }
done

echo "== EN pages without zh-cn twin / zh-cn without EN =="
(cd "$root" && find . -path ./zh-cn -prune -o -type f -print | sed 's|^\./||' | sort) > /tmp/docs-en.txt
(cd "$root/zh-cn" && find . -type f | sed 's|^\./||' | sort) > /tmp/docs-zh.txt
diff /tmp/docs-en.txt /tmp/docs-zh.txt | sed 's/^/  /' || status=1

echo "== forbidden references =="
grep -rnE 'AGENTS\.md|CLAUDE\.md|design/[a-z-]+\.md|releases/latest/download/gproxy-|GPROXY_SECRET_KEY|crates\.io/crates/gproxy' "$root" | sed 's/^/  /' && status=1

echo "== frontmatter present =="
for f in $(find "$root" -type f); do head -1 "$f" | grep -q '^---$' || { echo "  no frontmatter: $f"; status=1; }; done

echo "== pages over 320 lines =="
find "$root" -type f -exec wc -l {} + | sort -rn | awk '$1 > 320 && $2 != "total" {print "  " $0}'

echo "== totals =="
echo "  EN: $(wc -l < /tmp/docs-en.txt) files, zh-cn: $(wc -l < /tmp/docs-zh.txt) files"
exit $status
